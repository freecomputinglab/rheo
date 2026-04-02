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

smoke_tests! {
    (maths_ohrg_org,            "https://github.com/freecomputinglab/maths.ohrg.org"),
    (rheo_ohrg_org,             "https://github.com/freecomputinglab/rheo.ohrg.org"),
    (freecomputinglab_ohrg_org, "https://github.com/freecomputinglab/freecomputinglab.ohrg.org"),
    (lolm_ohrg_org,             "https://github.com/freecomputinglab/lolm.ohrg.org"),
    (digitaltheory_dot_org,     "https://github.com/digitaltheorylab/digitaltheory-dot-org"),
}
