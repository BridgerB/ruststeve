//! A run's evidence while it runs (issue 16, docs/problems/race-tooling.md): the ledger, the
//! senses and the story so far as text beside the progress file, rewritten at every progress
//! interval, so a race is read while it runs and a lane that never finishes still leaves them.
//! The run's end writes the same files whole (bin/race.rs).

use std::path::{Path, PathBuf};

/// What one writing needs: copies, so the loop thread never waits on the disk.
pub struct Evidence {
    pub dir: PathBuf,
    pub run: String,
    pub bot: String,
    /// The progress line of this moment: the first line of the case text.
    pub progress: String,
    pub tasks: Vec<plan::TaskRecord>,
    pub facts: Vec<observe::Stamped>,
    pub senses: Vec<observe::Senses>,
}

/// The case text of a run still going: its progress line, then the story so far.
pub fn running_case(progress: &str, story: &[String]) -> String {
    let mut out = format!(
        "RUNNING (not the run's case: written while it runs, replaced when it ends)\n{}\n",
        progress.trim_end()
    );
    for line in story {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// A whole file or none: written beside itself and renamed, so a reader (the lane's uploader)
/// never takes half of one.
fn write_whole(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

impl Evidence {
    /// Writes `<bot>.ledger.txt`, `<bot>.senses.txt` and `<bot>.case.txt`.
    pub fn write(self) -> Result<(), String> {
        let ledger = results::Ledger {
            format_version: results::FORMAT_VERSION,
            run: self.run.clone(),
            bot: self.bot.clone(),
            commit: record::COMMIT.to_owned(),
            tasks: self.tasks,
            dropped: 0,
            facts: self.facts,
        };
        let file = |kind: &str| self.dir.join(format!("{}.{kind}.txt", self.bot));
        write_whole(&file("ledger"), &ledger.render())?;
        let story = results::story::story(&ledger, &self.senses).render();
        write_whole(&file("case"), &running_case(&self.progress, &story))?;
        let senses = results::SensesFile {
            format_version: results::FORMAT_VERSION,
            run: self.run,
            bot: self.bot.clone(),
            rows: self.senses,
        };
        write_whole(&file("senses"), &senses.render(10))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_case_says_it_is_not_the_case() {
        let text = running_case("rs-l00-b00\ttick 2400\n", &["stages".to_owned()]);
        let mut lines = text.lines();
        assert!(lines.next().unwrap().starts_with("RUNNING"));
        assert_eq!(lines.next(), Some("rs-l00-b00\ttick 2400"));
        assert_eq!(lines.next(), Some("stages"));
    }

    #[test]
    fn the_three_files_are_written_whole() {
        let dir = std::env::temp_dir().join(format!("ruststeve-evidence-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let e = Evidence {
            dir: dir.clone(),
            run: "local".to_owned(),
            bot: "rs-l00-b00".to_owned(),
            progress: "rs-l00-b00\ttick 2400".to_owned(),
            tasks: vec![plan::TaskRecord::Note {
                at: 2300,
                what: "sealed the lava at the door".to_owned(),
                why: "lava beside the head".to_owned(),
            }],
            facts: Vec::new(),
            senses: Vec::new(),
        };
        e.write().unwrap();
        let read = |kind: &str| {
            std::fs::read_to_string(dir.join(format!("rs-l00-b00.{kind}.txt"))).unwrap()
        };
        assert!(read("ledger").contains("sealed the lava at the door"));
        assert!(read("case").starts_with("RUNNING"));
        assert!(read("senses").starts_with("senses "));
        assert!(!dir.join("rs-l00-b00.ledger.part").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
