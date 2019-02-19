lazy_static::lazy_static! {
    static ref INPUT_OUTPUT_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
}

fn input_output(args: &[impl AsRef<std::ffi::OsStr>], input: &[u8], output: &[u8]) {
    let _lock = INPUT_OUTPUT_MUTEX.lock();
    let mut input_file = std::fs::File::create("test_input").unwrap();
    std::io::Write::write_all(&mut input_file, input).unwrap();
    std::mem::drop(input);
    let input_file = std::fs::File::open("test_input").unwrap();
    let real_output = std::process::Command::new("target/debug/cut")
        .args(args)
        .stdin(input_file)
        .output()
        .unwrap();
    assert_eq!(&real_output.stdout[..], output);
    std::fs::remove_file("test_input").unwrap();
}

#[test]
fn simple1() {
    input_output(&["-d,", "-f2"], b"a,b,c\nd,e,f", b"b\ne\n");
}

#[test]
fn simple2() {
    input_output(
        &["-z", "-b=2,2,2"],
        b"\xff\xfe\xfd\0abc\0",
        b"\xfe\xfe\xfe\0bbb\0",
    )
}

#[test]
fn simple3() {
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
