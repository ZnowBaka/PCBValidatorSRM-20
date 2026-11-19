use crate::commands::Cmd::*;
use crate::commands::Steps;
use crate::commands::render;

use std::fs::File;
use std::io::prelude::*;

pub mod commands;

fn main() -> std::io::Result<()> {
    let mut file = File::create("test.rml")?;
    file.write_all(render(&[
        Init, 
        ZHeights{cut: Steps(0),travel: Steps(1000)},
        SpeedXy(10.0),
        SpeedZ(1.0),
        Spindle(true),
        CutTo(Steps(1000), Steps(0)),
        CutTo(Steps(1000), Steps(1000)),
        CutTo(Steps(0), Steps(1000)),
        CutTo(Steps(0), Steps(0)),
        Spindle(false),
        ]).as_bytes())?;
    Ok(())
}