fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let n_str = if args.len() == 1 {
        &args[0]
    } else {
        eprint!("Expected 1 argument: <n>");
        std::process::exit(1);
    };

    let n: u32 = match n_str.parse() {
        Ok(v) => v,
        Err(_) => {
            eprint!("invalid number: {}", n_str);
            std::process::exit(1);
        }
    };

    let res = fib(n);
    print!("fib({n}) = {res}");
}

fn fib(n: u32) -> u32 {
    if n <= 1 {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}