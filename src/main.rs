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
        ZHeights{cut: Steps::from_mm(0.0),travel: Steps::from_mm(10.0)},
        SpeedXy(10.0),
        SpeedZ(1.0),
        CutTo(Steps::from_mm(10.0), Steps::from_mm(0.0)),
        CutTo(Steps::from_mm(10.0), Steps::from_mm(10.0)),
        Dwell(1000),
        CutTo(Steps::from_mm(0.0), Steps::from_mm(10.0)),
        CutTo(Steps::from_mm(0.0), Steps::from_mm(0.0)),
        Home
        ]).as_bytes())?;
    Ok(())
}