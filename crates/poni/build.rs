fn main() {
    println!("cargo:rerun-if-changed=res/poni.rc");
    println!("cargo:rerun-if-changed=res/poni.manifest");
    println!("cargo:rerun-if-changed=res/poni.ico");
    
    // Only Windows targets get the icon, manifest (Per-Monitor-V2 DPI) and version info.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let res_dir = std::path::Path::new(&manifest_dir).join("res");
        let out_obj = res_dir.join("poni_res.o");

        let windres_cmd = if std::path::Path::new(r"E:\Development\w64devkit\bin\windres.exe").exists() {
            r"E:\Development\w64devkit\bin\windres.exe".to_string()
        } else {
            "windres".to_string()
        };

        let status = std::process::Command::new(&windres_cmd)
            .current_dir(&res_dir)
            .args(&["-i", "poni.rc", "-o", "poni_res.o", "-O", "coff", "-F", "pe-x86-64"])
            .status();

        if let Ok(st) = status {
            if st.success() {
                println!("cargo:rustc-link-arg-bins={}", out_obj.display());
            }
        }

        // MSVC builds: resolve statically imported non-KnownDLLs (d2d1, dwrite,
        // dwmapi, winmm) from System32 only, never from the exe's folder.
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
        }
    }
    build_info();
}

fn build_info() {
    for var in ["GITHUB_ACTIONS", "GITHUB_REF_TYPE", "GITHUB_REF_NAME", "GITHUB_SHA"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let ci = env("GITHUB_ACTIONS") == "true";
    let release = ci && env("GITHUB_REF_TYPE") == "tag" && env("GITHUB_REF_NAME") == format!("v{}", env("CARGO_PKG_VERSION"));
    let kind = if release { "release" } else if ci { "test" } else { "local" };
    let commit: String = if ci { env("GITHUB_SHA").chars().take(7).collect() } else { String::new() };
    let day = if ci { utc_day() } else { String::new() };
    println!("cargo:rustc-env=PONI_BUILD_KIND={kind}");
    println!("cargo:rustc-env=PONI_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=PONI_BUILD_DAY={day}");
}

fn utc_day() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{d} {} {y}", MONTHS[(m - 1) as usize])
}
