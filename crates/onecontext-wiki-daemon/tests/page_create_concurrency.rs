use onecontext_wiki_core::WikiCore;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

#[test]
fn parallel_page_create_processes_preserve_registry_records() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("1Context");
    seed_runtime(&root);

    let exe = env!("CARGO_BIN_EXE_onecontext-wiki");
    let mut children = Vec::new();
    let page_count = 24;
    for n in 1..=page_count {
        let id = format!("race-page-{n:03}");
        let title = format!("Race Page {n:03}");
        let child = Command::new(exe)
            .arg("--root")
            .arg(&root)
            .arg("page-create")
            .arg(&id)
            .arg("--title")
            .arg(&title)
            .arg("--family-group")
            .arg("race")
            .arg("--family-group-title")
            .arg("Race")
            .arg("--family-id")
            .arg(&id)
            .arg("--family-title")
            .arg(&title)
            .arg("--type")
            .arg("context-page")
            .arg("--template")
            .arg("pages/context-page.md")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        children.push((id, child));
    }

    let mut expected = BTreeSet::new();
    for (id, child) in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "page-create {id} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        expected.insert(id);
    }

    let pages = WikiCore::new(&root).pages().unwrap();
    let actual = pages
        .iter()
        .filter(|page| page.id.starts_with("race-page-"))
        .map(|page| page.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);

    let wiki_toml = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
    for id in expected {
        assert!(
            root.join(format!(
                "user-wiki/source/families/race/{id}/source/{id}.md"
            ))
            .is_file(),
            "missing source for {id}"
        );
        assert!(wiki_toml.contains(&format!("id = \"{id}\"")));
    }
}

fn seed_runtime(root: &Path) {
    fs::create_dir_all(root.join("user-wiki/templates/pages")).unwrap();
    fs::create_dir_all(root.join("user-wiki/templates/talk")).unwrap();
    fs::write(
        root.join("user-wiki/wiki.toml"),
        r#"
schema_version = 1

[site]
navigation = ["topics"]
primary_navigation = ["topics"]
utility_navigation = []

[defaults]
operator_name = "Operator"
access_tier = "private"
asset_base = "."
home_href = "/"

[[pages]]
id = "topics"
enabled = true
title = "Topics"
slug = "topics"
route = "/topics"
family_group = "reference"
family_group_title = "Reference"
family_id = "topics"
family_title = "Topics"
type = "topic-index"
template = "pages/context-page.md"
talk_conventions_template = "talk/conventions.md"
"#,
    )
    .unwrap();
    fs::write(
        root.join("user-wiki/templates/pages/context-page.md"),
        "---\npage_id: \"{{ page_id }}\"\ntitle: \"{{ title }}\"\nslug: \"{{ slug }}\"\nroute: \"{{ route }}\"\nsection: \"{{ section }}\"\naccess: \"{{ access_tier }}\"\n---\n\n# {{ title }}\n",
    )
    .unwrap();
    fs::write(
        root.join("user-wiki/templates/talk/conventions.md"),
        "# {{ title }} Talk Conventions\n",
    )
    .unwrap();
}
