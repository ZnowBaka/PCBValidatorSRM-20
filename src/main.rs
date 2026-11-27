mod parser;
use anyhow::Error;

use crate::parser::gerber::{grb_parse, interpret};

pub mod zip_reader;
mod svg;

fn main() -> Result<(), Error> {
    let docs = grb_parse(r"C:\Users\user\Documents\rust\cam\PCBForTesing_v1_Manufacturing_Output.zip".to_string()).unwrap();
    let outline = interpret(&docs[0]).map_err(Error::msg)?;
    let copper  = interpret(&docs[2]).map_err(Error::msg)?;
    svg::write_svg("debug.svg", &[
        ("#b87333", copper.as_slice()),
        ("#0066cc", outline.as_slice()),
    ])?;
    Ok(())
}
