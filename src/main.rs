use std::env;
use std::error::Error;
use std::process;

use zipoxide::Zipctx;

fn main() -> Result<(), Box<dyn Error>>{
    let args: Vec<String> = env::args().collect();
    if args.len() != 2{
        eprintln!("Enter only the filename as argument");
        process::exit(1);
    }

    let zipfile = &args[1];
    let result : Zipctx = zipoxide::construct_zip(zipfile)
        .expect("Couldn't parse EOCD");

    Ok(())
}
