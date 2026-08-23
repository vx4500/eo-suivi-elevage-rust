#[test]
fn version_affichee_et_artifact_sont_synchronises() {
    let version = env!("CARGO_PKG_VERSION");
    let readme = include_str!("../README.md");
    let state = include_str!("../Etat-projet-eo-suivi-rust.md");
    let changelog = include_str!("../templates/correctifs.html");
    let workflow = include_str!("../.github/workflows/rust.yml");

    assert!(readme.starts_with(&format!("# EO-Suivi Élevage — portage Rust {version}")));
    assert!(state.contains(&format!("Version actuelle : **{version}**")));
    assert!(state.contains(&format!("Version Rust {version}")));
    assert!(changelog.contains(&format!("<h2>{version} —")));
    assert!(workflow.contains(&format!("eo-suivi-elevage-{version}-linux-x86_64-musl")));
}

#[test]
fn documentation_ne_renvoie_plus_vers_les_fichiers_supprimes() {
    let documents = [
        include_str!("../README.md"),
        include_str!("../Etat-projet-eo-suivi-rust.md"),
        include_str!("../templates/correctifs.html"),
    ];
    let deleted = [
        "VERSIONS-RUST.md",
        "MISE-A-JOUR-DEBIAN13.md",
        "AUDIT-PORTAGE-RUST-2.1.2.md",
    ];

    for document in documents {
        for filename in deleted {
            assert!(
                !document.contains(filename),
                "référence obsolète : {filename}"
            );
        }
    }
}

#[test]
fn saisie_rapide_prepare_le_sevrage_avant_lenvoi() {
    let template = include_str!("../templates/base.html");
    let submit = template
        .find("form.addEventListener('submit'")
        .expect("gestionnaire d'envoi de la saisie rapide");
    let script = &template[submit..];
    let serialisation = script
        .find("sevrage_truies').value=JSON.stringify(selected)")
        .expect("sérialisation des truies sélectionnées");
    let form_data = script
        .find("var fd = new FormData(form)")
        .expect("construction des données du formulaire");

    assert!(
        serialisation < form_data,
        "les truies doivent être sérialisées avant FormData"
    );
    assert!(template.contains("sow.disabled=movement||sevrage"));
    assert!(template.contains("selected=sevrageWholeSelection.slice()"));
    assert!(!template.contains("inp.previousSibling.checked"));
}
