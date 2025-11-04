use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    
    println!("Key Debug Tool - Press keys to see what's detected\r");
    println!("Press 'q' to quit\r");
    println!("---\r");
    
    loop {
        if let Event::Key(key) = event::read()? {
            let mods = format!(
                "{}{}{}",
                if key.modifiers.contains(KeyModifiers::CONTROL) { "Ctrl+" } else { "" },
                if key.modifiers.contains(KeyModifiers::ALT) { "Alt+" } else { "" },
                if key.modifiers.contains(KeyModifiers::SHIFT) { "Shift+" } else { "" }
            );
            
            println!("Key: {:?} | Mods: {} | Raw: {:?}\r", key.code, mods, key);
            
            if let KeyCode::Char('q') = key.code {
                if !key.modifiers.contains(KeyModifiers::ALT) {
                    break;
                }
            }
        }
    }
    
    disable_raw_mode()?;
    println!("\nExiting...");
    Ok(())
}
