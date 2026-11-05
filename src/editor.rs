// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::DirBuilder;
use std::io::ErrorKind;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::Result;

pub const BUILTIN: &str = "builtin";
const BUILTIN_BINARY: &str = "accioshot-editor";
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Opens the capture in an editor. Returns the edited image, or None if the user discarded it.
pub fn edit(command: &str, png: &[u8]) -> Result<Option<Vec<u8>>> {
    let dir = dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("accioshot-edit-{}", std::process::id()));
    // 0700: this is a screenshot, it may hold anything.
    DirBuilder::new().mode(0o700).create(&dir)?;
    let result = edit_in(command, png, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn edit_in(command: &str, png: &[u8], dir: &Path) -> Result<Option<Vec<u8>>> {
    let input = dir.join("input.png");
    let output = dir.join("output.png");
    std::fs::write(&input, png)?;

    let plan = plan(command, &input, &output)?;
    let status = Command::new(&plan.program)
        .args(&plan.args)
        .stdin(Stdio::null())
        // Our stdout is the result scripts read.
        .stdout(Stdio::null())
        .status()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => format!(
                "editor {} not found, check `command` in [editor]",
                plan.program.display()
            ),
            _ => format!("cannot run the editor: {error}"),
        })?;
    if !status.success() {
        return Err(format!("the editor failed: {status}").into());
    }

    let edited = match std::fs::read(&plan.result) {
        Ok(edited) if edited.is_empty() => return Ok(None),
        Ok(edited) => edited,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !edited.starts_with(PNG_SIGNATURE) {
        return Err("the editor did not write a PNG".into());
    }
    Ok(Some(edited))
}

#[derive(Debug, PartialEq)]
struct Plan {
    program: PathBuf,
    args: Vec<String>,
    result: PathBuf,
}

/// shortcut: templates split on whitespace, no quoting. The paths are replaced after splitting so
/// they can have spaces. Use a real parser if an editor needs quoted arguments.
fn plan(command: &str, input: &Path, output: &Path) -> Result<Plan> {
    let input_text = input.display().to_string();
    let output_text = output.display().to_string();
    if command == BUILTIN {
        return Ok(Plan {
            program: builtin_binary(),
            args: vec![input_text, output_text],
            result: output.to_path_buf(),
        });
    }
    let mut parts = command.split_whitespace().map(|part| {
        part.replace("{input}", &input_text)
            .replace("{output}", &output_text)
    });
    let program = parts.next().ok_or("`command` in [editor] is empty")?;
    Ok(Plan {
        program: program.into(),
        args: parts.collect(),
        // Without {output} the editor must change the file in place.
        result: match command.contains("{output}") {
            true => output.to_path_buf(),
            false => input.to_path_buf(),
        },
    })
}

/// Next to this binary first, so a build or a package finds its own editor.
fn builtin_binary() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(BUILTIN_BINARY)))
        .filter(|path| path.exists())
        .unwrap_or_else(|| BUILTIN_BINARY.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(command: &str, png: &[u8]) -> Result<Option<Vec<u8>>> {
        let dir = std::env::temp_dir().join(format!(
            "accioshot-editor-test-{}-{:x}",
            std::process::id(),
            command.len() ^ png.len() << 8
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let result = edit_in(command, png, &dir);
        std::fs::remove_dir_all(&dir).unwrap();
        result
    }

    #[test]
    fn templates_fill_paths_even_with_spaces() {
        let plan = plan(
            "satty --filename {input} --output-filename {output}",
            Path::new("/a b/in.png"),
            Path::new("/a b/out.png"),
        )
        .unwrap();
        assert_eq!(plan.program, PathBuf::from("satty"));
        assert_eq!(
            plan.args,
            [
                "--filename",
                "/a b/in.png",
                "--output-filename",
                "/a b/out.png"
            ]
        );
        assert_eq!(plan.result, PathBuf::from("/a b/out.png"));
    }

    #[test]
    fn without_output_the_file_is_edited_in_place() {
        let plan = plan("gimp {input}", Path::new("/in"), Path::new("/out")).unwrap();
        assert_eq!(plan.result, PathBuf::from("/in"));
    }

    #[test]
    fn empty_command_fails() {
        assert!(plan("  ", Path::new("/in"), Path::new("/out")).is_err());
    }

    #[test]
    fn output_is_returned() {
        let png = [PNG_SIGNATURE, b"edited"].concat();
        let edited = run("cp {input} {output}", &png).unwrap();
        assert_eq!(edited, Some(png));
    }

    #[test]
    fn no_output_means_discarded() {
        assert_eq!(run("true {input} {output}", PNG_SIGNATURE).unwrap(), None);
    }

    #[test]
    fn editor_failure_is_an_error() {
        assert!(run("false {output}", PNG_SIGNATURE).is_err());
    }

    #[test]
    fn missing_editor_says_so() {
        let error = run("accioshot-no-such-editor", PNG_SIGNATURE).unwrap_err();
        assert!(error.to_string().contains("not found"));
    }

    #[test]
    fn output_must_be_a_png() {
        assert!(run("cp {input} {output}", b"not a png").is_err());
    }
}
