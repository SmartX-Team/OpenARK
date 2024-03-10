use dash_pipe_function_python_provider::Function;
use dash_pipe_provider::PipeArgs;

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}
