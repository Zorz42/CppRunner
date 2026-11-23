use cpprunner::{COMPILER_ENV_VAR, Compiler, Error};

/// The only test in this binary, so changing the environment races with nothing.
#[test]
fn env_var_selects_the_compiler() {
    let set = |value: &str| unsafe { std::env::set_var(COMPILER_ENV_VAR, value) };
    let fallback = {
        unsafe {
            std::env::remove_var(COMPILER_ENV_VAR);
        }
        Compiler::detect().unwrap().path().to_path_buf()
    };

    set("");
    assert_eq!(Compiler::detect().unwrap().path(), fallback);

    set(&fallback.to_string_lossy());
    assert_eq!(Compiler::detect().unwrap().path(), fallback);

    let name = fallback.file_name().unwrap().to_string_lossy().into_owned();
    set(&name);
    assert_eq!(Compiler::detect().unwrap().path().file_name().unwrap().to_string_lossy(), name);

    set("no-such-compiler-cpprunner");
    assert!(matches!(Compiler::detect(), Err(Error::CompilerNotFound)));

    set("/no/such/dir/g++");
    assert!(matches!(Compiler::detect(), Err(Error::CompilerNotFound)));

    unsafe {
        std::env::remove_var(COMPILER_ENV_VAR);
    }
}
