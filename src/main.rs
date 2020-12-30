use structopt::StructOpt;
use prom2jsonrs::PrometheusData;

#[derive(StructOpt)]
struct Cli {
    // url to query for prom metrics
    url: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::from_args();
    let resp = reqwest::blocking::get(&args.url)?.text()?;
    println!("{}", serde_json::to_string(&PrometheusData::from_string(&resp)).unwrap());
    Ok(())
}

