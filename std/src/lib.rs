#[no_mangle]
pub extern "C" fn print_byte(num: u8) {
    use std::io::Write;

    print!("{}", num as char);
    std::io::stdout().flush().unwrap();
}

#[no_mangle]
pub extern "C" fn read_byte() -> u8 {
    let out = console::Term::stdout();
    out.read_char().unwrap() as u8
}
