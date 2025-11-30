use anyhow::{ensure, Result};
use rheo::{cli::Commands, Cli};
use std::{env::set_current_dir, fs, io, path::Path, process::Command};
use tempfile::TempDir;
use walkdir::WalkDir;

fn copy_all(src: &Path, dst: &Path) -> io::Result<()> {
    for src_entry in WalkDir::new(src).into_iter().skip(1) {
        let src_entry = src_entry?;
        let src_abs_path = src_entry.path();
        let src_rel_path = src_abs_path.strip_prefix(src).unwrap();
        let dst_path = dst.join(src_rel_path);
        if src_abs_path.is_dir() {
            fs::create_dir(dst_path)?;
        } else {
            fs::copy(src_abs_path, dst_path)?;
        }
    }
    Ok(())
}

fn epubcheck(path: &Path) -> Result<()> {
    let status = Command::new("epubcheck").arg(path).status()?;
    ensure!(status.success(), "epubcheck failed");
    Ok(())
}

#[test]
fn integration() -> Result<()> {
    let inputs = fs::read_dir("tests/inputs")?;

    for input in inputs {
        let input = input?.path();
        let tmpdir = TempDir::new()?;
        let tmp_path = tmpdir.keep();
        eprintln!("{}", tmp_path.display());

        let project_name = input.file_name().unwrap();
        let src = tmp_path.join(project_name);
        fs::create_dir(&src)?;

        let build = tmp_path.join("build");
        fs::create_dir(&build)?;

        copy_all(&input, &src)?;

        set_current_dir(tmp_path)?;
        Cli {
            quiet: true,
            verbose: false,
            command: Commands::Compile {
                path: src,
                pdf: true,
                html: true,
                epub: true,
            },
        }
        .run()?;

        let epub_path = build
            .join(project_name)
            .join("epub")
            .join(input.with_extension("epub").file_name().unwrap());
        epubcheck(&epub_path)?;
    }

    Ok(())
}
