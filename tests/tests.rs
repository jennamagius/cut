fn input_output(args: &[impl AsRef<std::ffi::OsStr>], input: &[u8], output: &[u8]) {
    let mut input_file = tempfile::tempfile().unwrap();
    std::io::Write::write_all(&mut input_file, input).unwrap();
    std::io::Seek::seek(&mut input_file, std::io::SeekFrom::Start(0)).unwrap();
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    #[cfg(debug_assertions)]
    path.push("debug");
    #[cfg(not(debug_assertions))]
    path.push("release");
    path.push(env!("CARGO_PKG_NAME"));
    let real_output = std::process::Command::new(&path)
        .args(args)
        .stdin(input_file)
        .output()
        .unwrap();
    assert_eq!(&real_output.stdout[..], output);
}

#[test]
fn simple1() {
    input_output(&["-d,", "-f2"], b"a,b,c\nd,e,f", b"b\ne\n");
}

#[test]
fn zero_terminated() {
    input_output(
        &["-z", "-b=2,2,2"],
        b"\xff\xfe\xfd\0abc\0",
        b"\xfe\xfe\xfe\0bbb\0",
    )
}

#[test]
fn complement() {
    input_output(
        &["--complement", "-f", "2,4"],
        b"a\t \tb   c\td\n",
        b"a\tc\n",
    )
}

#[test]
fn widechar() {
    input_output(&["-c", "2"], "💩😀💩".as_bytes(), "😀\n".as_bytes());
}

#[test]
fn reverse() {
    input_output(&["-b4-2"], b"abcde\n", b"dcb\n")
}

#[test]
fn only_delimited() {
    input_output(
        &["--only-delimited", "-d", "banana", "-f-", "-j "],
        b"abananabbananac\na b c d\nqbananarbanana",
        b"a b c\nq r \n",
    );
}

#[test]
fn tab_delimited() {
    input_output(&["-t", "-f~", "-j "], b"a\t \t b\tc", b"c  b   a\n")
}

#[test]
fn zero_delimited() {
    input_output(&["-Z", "-f-", "-j", ".."], b"a\0b\0c", b"a..b..c\n")
}
