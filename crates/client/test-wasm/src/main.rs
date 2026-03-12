fn main() {
    let arg = std::env::args().nth(1);
    let n_str = match arg {
        Some(s) => s,
        None => {
            eprintln!("Usage: <program> <n>");
            std::process::exit(2);
        }
    };

    let n: u32 = match n_str.parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("invalid number: {}", n_str);
            std::process::exit(2);
        }
    };

    let res = fib(n);
    println!("fib({n}) = {res}");
}

fn fib(n: u32) -> u32 {
    if n <= 1 {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}