use rheo_tests::helpers::remote::run_compat;

fn compat_enabled() -> bool {
    std::env::var("RUN_COMPAT_TESTS").as_deref() == Ok("1")
}

macro_rules! smoke_tests {
    ( $( ($name:ident, $url:expr) ),* $(,)? ) => {
        $(
            ::paste::paste! {
                #[test]
                fn [<smoke_ $name>]() {
                    if !compat_enabled() { return; }
                    run_compat($url, stringify!($name));
                }
            }
        )*
    };
}

// Repos are registered in rheo-3cr
smoke_tests! {}
