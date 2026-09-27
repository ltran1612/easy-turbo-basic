//! Stands in for an interactive built program.
//!
//! The point of this binary is the first `print!`: a prompt with **no trailing
//! newline**, exactly like BASIC's `INPUT "N";N`. If anything in the
//! reading path is line-oriented, that prompt never reaches the user and the
//! program appears to hang — which, for a program waiting on READ, it does forever.

use std::io::{BufRead, Write};

fn main() {
    print!("Nhap so N: ");
    std::io::stdout().flush().unwrap();

    let mut line = String::new();
    let n = std::io::stdin().lock().read_line(&mut line).unwrap_or(0);
    if n == 0 {
        // EOF before any input.
        println!();
        println!("Ket thuc (EOF)");
        std::process::exit(4);
    }

    let value = line.trim();
    println!();
    println!("Ket qua = {value}");
    std::process::exit(3);
}
