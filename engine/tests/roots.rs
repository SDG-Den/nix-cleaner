use engine::model::{Protection, Root, RootKind};
use engine::roots::apply_protection;

fn generation(number: u32) -> Root {
    Root {
        link: format!("/nix/var/nix/profiles/system-{number}-link"),
        target: format!("/nix/store/system-{number}"),
        kind: RootKind::SystemGeneration,
        label: format!("System Generation {number}"),
        generation: Some(number),
        protection: Protection::None,
    }
}

fn current(kind: RootKind, target: &str) -> Root {
    Root {
        link: "/nix/var/nix/profiles/system".to_string(),
        target: target.to_string(),
        kind,
        label: "Current System".to_string(),
        generation: None,
        protection: Protection::None,
    }
}

#[test]
fn newest_three_and_current_are_protected() {
    let mut roots = vec![
        generation(1),
        generation(2),
        generation(3),
        generation(4),
        generation(5),
        current(RootKind::SystemCurrent, "/nix/store/system-5"),
    ];
    apply_protection(&mut roots);

    assert_eq!(roots[0].protection, Protection::None);
    assert_eq!(roots[1].protection, Protection::None);
    assert_eq!(roots[2].protection, Protection::NewestGeneration);
    assert_eq!(roots[3].protection, Protection::NewestGeneration);
    assert_eq!(roots[4].protection, Protection::CurrentGeneration);
    assert_eq!(roots[5].protection, Protection::CurrentSystem);
}

#[test]
fn booted_generation_is_protected_even_when_old() {
    let mut roots = vec![
        generation(1),
        generation(2),
        generation(3),
        generation(4),
        generation(5),
        current(RootKind::BootedSystem, "/nix/store/system-1"),
    ];
    apply_protection(&mut roots);
    assert_eq!(roots[0].protection, Protection::CurrentGeneration);
}

#[test]
fn system_owned_gcroots_cannot_be_selected() {
    let mut roots = vec![Root {
        link: "/nix/var/nix/gcroots/auto/abc".to_string(),
        target: "/nix/store/x".to_string(),
        kind: RootKind::Gcroot,
        label: "GC root".to_string(),
        generation: None,
        protection: Protection::None,
    }];
    apply_protection(&mut roots);
    assert_eq!(roots[0].protection, Protection::SystemOwned);
}
