use std::io::{BufRead, Write};

pub fn ask(question: &str) -> anyhow::Result<String> {
    eprint!("{question} ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(answer.trim().to_owned())
}

pub fn confirm(word: &str) -> anyhow::Result<bool> {
    Ok(ask(&format!("Type `{word}` to confirm:"))? == word)
}
