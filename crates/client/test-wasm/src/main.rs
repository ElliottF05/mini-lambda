use std::{thread::sleep, time::Duration};

fn main() {
    let arg = std::env::args().nth(1);
    let n_str = match arg {
        Some(s) => s,
        None => {
            eprint!("Usage: <program> <n>");
            std::process::exit(1);
        }
    };

    let n: u32 = match n_str.parse() {
        Ok(v) => v,
        Err(_) => {
            eprint!("invalid number: {}", n_str);
            std::process::exit(1);
        }
    };
    
    // let res = fib(n);
    // print!("fib({n}) = {res}");

    sleep(Duration::from_secs(n.into()));
    print!("slept for {} seconds", n);

}

// fn fib(n: u32) -> u32 {
//     if n <= 1 {
//         n
//     } else {
//         fib(n - 1) + fib(n - 2)
//     }
// }