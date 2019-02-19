use std::os::unix::ffi::OsStrExt;

fn clap_app() -> clap::App<'static, 'static> {
    use clap::Arg;
    clap::App::new("cut")
        .version(clap::crate_version!())
        .arg(
            Arg::with_name("delimiter")
                .takes_value(true)
                .help("What to split on. If not specified, splits on contiguous hunks of whitespace characters. Can be multiple characters and non-utf8. Input mut be utf8 if this parameter is not specified, but may be non-utf8 otherwise.")
                .allow_hyphen_values(true)
                .short("d")
                .long("delimiter"),
        )
        .arg(
            Arg::with_name("fields")
                .takes_value(true)
                .help("Which fields to output. Fields can be repeated. Ranges can be backwards. Use the special value '~' to select all fields but in reverse order.")
                .allow_hyphen_values(true)
                .short("f")
                .long("fields"),
        )
        .arg(
            Arg::with_name("characters")
                .takes_value(true)
                .help("Which characters to output. Requires input lines to be utf8.")
                .allow_hyphen_values(true)
                .short("c")
                .long("characters"),
        )
        .arg(
            Arg::with_name("bytes")
                .takes_value(true)
                .help("Which bytes to output. Use special value '~' to make a slightly inefficient version of 'rev'.")
                .allow_hyphen_values(true)
                .short("b")
                .long("bytes"),
        )
        .arg(
            Arg::with_name("complement")
                .takes_value(false)
                .help("Output all the fields that weren't selected, in their original order, joined by joiner.")
                .long("complement"),
        )
        .arg(
            Arg::with_name("zero-terminated")
                .takes_value(false)
                .help("Lines end with \\0 instead of \\n")
                .short("z")
                .long("zero-terminated"),
        )
        .arg(Arg::with_name("zero-delimited").takes_value(false).short("Z").long("zero-delimited").help("A quick way to specify -d$(echo -e -n \0)").conflicts_with("delimiter"))
        .arg(Arg::with_name("tab-delimited").takes_value(false).short("t").long("tab-delimited").help("A quick way to specify -d$(echo -e -n \t)").conflicts_with("zero-delimited").conflicts_with("delimiter"))
        .arg(
            Arg::with_name("only-delimited")
                .takes_value(false)
                .help("Skip any lines that don't contain the delimiter. If no delimiter is provided but -f is being used, skip any lines that don't contain any whitespace. Has no effect for -c or -b")
                .short("s")
                .long("only-delimited"),
        )
        .arg(
            Arg::with_name("joiner")
                .takes_value(true)
                .help("The value to be inserted between selected fields. Defaults to the value of delimiter, \\t for -f without -d, or the empty string for -c and -b")
                .short("j")
                .long("joiner"),
        )
}

#[derive(Default, Debug)]
struct Range {
    start: Option<usize>,
    end: Option<usize>,
    inverting: bool,
}

impl Range {
    fn splitrange(range: &str, delim: &str) -> (Option<usize>, Option<usize>) {
        let mut iter = range.splitn(2, delim);
        (
            iter.next().unwrap().parse().ok(),
            iter.next().unwrap().parse().ok(),
        )
    }

    fn parse2(range: &str) -> Option<Range> {
        let mut result = Range::default();
        if range == "~" {
            result.inverting = true;
            return Some(result);
        }
        if range.contains("-") {
            let (start, end) = Range::splitrange(range, "-");
            if end.is_none() || start.is_none() || end >= start {
                result.start = start;
                result.end = end;
                result.inverting = false;
            } else {
                result.end = start;
                result.start = end;
                result.inverting = true;
            }
            Some(result)
        } else {
            let value = range.parse().ok()?;
            result.start = Some(value);
            result.end = Some(value);
            Some(result)
        }
    }

    fn parse(range: &str) -> Result<Range, String> {
        Range::parse2(range).ok_or(range.to_string())
    }

    fn bytes_join(selected: &[&[u8]], joiner: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        for item in selected.iter().take(selected.len().saturating_sub(1)) {
            result.extend(*item);
            result.extend(joiner);
        }
        if let Some(last) = selected.iter().last() {
            result.extend(*last);
        }
        result
    }

    fn select(ranges: &[Range], inputs: &[&[u8]], joiner: &[u8]) -> Vec<u8> {
        let mut selected = Vec::new();
        for range in ranges {
            selected.extend(range.select_one(inputs));
        }

        Range::bytes_join(&selected, joiner)
    }

    fn select_one<'a>(&self, inputs: &[&'a [u8]]) -> Vec<&'a [u8]> {
        let mut end = self.end.unwrap_or(inputs.len());
        let start = self.start.unwrap_or(1);
        if end > inputs.len() {
            end = inputs.len();
        }
        if start > end {
            return Vec::new();
        }
        if self.inverting {
            inputs[(start - 1)..end]
                .into_iter()
                .map(|x| *x)
                .rev()
                .collect()
        } else {
            inputs[(start - 1)..end].to_vec()
        }
    }

    fn select_complement(ranges: &[Range], inputs: &[&[u8]], joiner: &[u8]) -> Vec<u8> {
        let mut selected: Vec<Option<&[u8]>> = inputs.iter().map(|x| Some(*x)).collect();
        for range in ranges {
            range.select_complement_one(&mut selected);
        }
        let selected: Vec<&[u8]> = selected
            .into_iter()
            .filter(|x| x.is_some())
            .map(|x| x.unwrap())
            .collect();
        Range::bytes_join(&selected, joiner)
    }

    fn select_complement_one(&self, inputs: &mut [Option<&[u8]>]) {
        let mut end = self.end.unwrap_or(inputs.len());
        let start = self.start.unwrap_or(1);
        if end > inputs.len() {
            end = inputs.len();
        }
        if start > end {
            return;
        }
        for i in &mut inputs[(start - 1)..end] {
            *i = None;
        }
    }
}

#[derive(PartialEq)]
enum Mode {
    Fields,
    Bytes,
    Characters,
}

fn read_line<T: std::io::Read>(input: &mut T, delim: u8) -> Option<Vec<u8>> {
    let mut buf = [0u8];
    let mut result = Vec::new();
    loop {
        let read_result = std::io::Read::read(input, &mut buf);
        if read_result.is_err() {
            if result.is_empty() {
                return None;
            } else {
                return Some(result);
            }
        }
        if read_result.unwrap() != 1 {
            if result.is_empty() {
                return None;
            } else {
                return Some(result);
            }
        }
        if buf[0] == delim {
            return Some(result);
        }
        result.extend(&buf);
    }
}

fn main() {
    let matches = clap_app().get_matches();
    let (ranges, mode) = match (
        matches.is_present("fields"),
        matches.is_present("bytes"),
        matches.is_present("characters"),
    ) {
        (true, false, false) => (matches.value_of("fields").unwrap(), Mode::Fields),
        (false, true, false) => (matches.value_of("bytes").unwrap(), Mode::Bytes),
        (false, false, true) => (matches.value_of("characters").unwrap(), Mode::Characters),
        _ => {
            eprintln!("You must specify precisely one of fields, bytes, or characters.");
            std::process::exit(1);
        }
    };
    let delimiter = match (
        matches.is_present("delimiter"),
        matches.is_present("zero-delimited"),
        matches.is_present("tab-delimited"),
    ) {
        (true, false, false) => Some(matches.value_of_os("delimiter").unwrap().as_bytes()),
        (false, true, false) => Some(&b"\0"[..]),
        (false, false, true) => Some(&b"\t"[..]),
        _ => None,
    };
    let ranges: Vec<Range> = ranges
        .split(",")
        .map(|x| {
            Range::parse(x)
                .map_err(|e| {
                    eprintln!("Unable to parse range: {}", e);
                    std::process::exit(1);
                })
                .unwrap()
        })
        .collect();
    let stdin = std::io::stdin();
    let mut lock = stdin.lock();
    let line_delim = if matches.is_present("zero-terminated") {
        '\0' as u8
    } else {
        '\n' as u8
    };
    let mut line_number: u64 = 0;
    while let Some(line) = read_line(&mut lock, line_delim) {
        line_number = line_number.checked_add(1).unwrap();
        let inputs: Vec<&[u8]> = match mode {
            Mode::Fields => {
                let result = if delimiter.is_some() {
                    let delimiter = delimiter.unwrap();
                    let mut cursor = 0;
                    let mut prev_cursor = 0;
                    let mut result = Vec::new();
                    let delimiter_len = delimiter.len();
                    let line_len = line.len();
                    while cursor + delimiter_len <= line_len {
                        if &line[cursor..cursor + delimiter_len] == delimiter {
                            result.push(&line[prev_cursor..cursor]);
                            cursor = cursor + delimiter_len;
                            prev_cursor = cursor;
                        } else {
                            cursor += 1;
                        }
                    }
                    result.push(&line[prev_cursor..]);
                    result
                } else {
                    let line_string = String::from_utf8(line.clone());
                    if line_string.is_err() {
                        eprintln!(
                            "Failed to process input line {} as string for whitespace split - invalid UTF-8",
                            line_number
                        );
                        continue;
                    }
                    let line_string = line_string.unwrap();
                    let entries: Vec<&str> = line_string.split_whitespace().collect();
                    entries
                        .into_iter()
                        .map(|q| {
                            let start_idx = q.as_ptr() as usize - line_string.as_ptr() as usize;
                            &line[start_idx..start_idx + q.len()]
                        })
                        .collect()
                };
                if matches.is_present("only-delimited") && result.len() <= 1 {
                    continue;
                }
                result
            }
            Mode::Bytes => (0..line.len()).map(|x| &line[x..x + 1]).collect(),
            Mode::Characters => {
                let mut result = Vec::new();
                let line_string = std::str::from_utf8(&line);
                if line_string.is_err() {
                    eprintln!(
                        "Failed to process input line {} as string for character split - invalid UTF-8",
                        line_number
                    );
                    continue;
                }
                let line_string = line_string.unwrap();
                let mut prev = 0;
                for current in line_string
                    .char_indices()
                    .map(|(a, _)| a)
                    .skip(1)
                    .chain([line.len()].to_vec())
                {
                    result.push(&line[prev..current]);
                    prev = current;
                }
                result
            }
        };
        let joiner = matches
            .value_of_os("joiner")
            .map(|x| x.as_bytes())
            .unwrap_or_else(|| delimiter.unwrap_or(if mode == Mode::Fields { b"\t" } else { b"" }));
        let stdout = std::io::stdout();
        let mut stdout_lock = stdout.lock();
        let mut result = if matches.is_present("complement") {
            Range::select_complement(&ranges, &inputs, &joiner)
        } else {
            Range::select(&ranges, &inputs, &joiner)
        };
        result.push(line_delim);
        std::io::Write::write_all(&mut stdout_lock, &result).unwrap();
    }
}
