fn main() {
    let acknowledgements = two_face::acknowledgement::listing();
    for license in acknowledgements.for_syntaxes() {
        println!("\n--- {} ---\n{}", license.rel_path.display(), license.text);
    }
}
