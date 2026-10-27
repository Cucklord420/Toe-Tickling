use std::{
    env, fs,
    io::{self, Write},
    process,
};

fn main() {
    match env::args().nth(1).as_deref() {
        Some("clean") => {
            mark_invocation("filter-probe-ran");
            let mut input = io::stdin().lock();
            let mut output = io::stdout().lock();
            io::copy(&mut input, &mut output).unwrap();
        }
        Some("process") => {
            mark_invocation("filter-process-probe-ran");
            process::exit(1);
        }
        Some("fsmonitor") => {
            io::stdout().write_all(b"noop\0").unwrap();
        }
        _ => process::exit(2),
    }
}

fn mark_invocation(name: &str) {
    fs::write(env::current_dir().unwrap().join(".git").join(name), b"ran").unwrap();
}
