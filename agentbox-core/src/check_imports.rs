use wasmtime_wasi::pipe;

fn main() {
    let _ = pipe::ReadPipe::from(vec![]);
}
