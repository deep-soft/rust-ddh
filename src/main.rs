use clap::{App, Arg};
use ddh::fileinfo::Fileinfo;
use rayon::prelude::*;
use std::fs::{self};
use std::io::prelude::*;
use std::io::stdin;
use std::path::PathBuf;

#[derive(Debug, Copy, Clone)]
pub enum PrintFmt {
    Standard,
    Json,
}

pub enum Verbosity {
    Quiet,
    Duplicates,
    All,
}

static DDH_ABOUT: &str = "Compare and contrast directories.\nExample invocation: ddh -d /home/jon/downloads /home/jon/documents -v duplicates\nExample pipe: ddh -d ~/Downloads/ -o no -v all -f json | someJsonParser.bin";

fn main() {
    let arguments = App::new("Directory Difference hTool")
                        .version(env!("CARGO_PKG_VERSION"))
                        .author(env!("CARGO_PKG_AUTHORS"))
                        .about(DDH_ABOUT)
                        .arg(Arg::new("directories")
                               .short('d')
                               .long("directories")
                               .value_name("Directories")
                               .help("Directories to parse")
                               .min_values(1)
                               .required(true)
                               .takes_value(true))
                        .arg(Arg::new("ignore")
                               .short('i')
                               .long("ignore")
                               .value_name("Ignore")
                               .help("Directories to ignore")
                               .min_values(1)
                               .required(false)
                               .takes_value(true))
                        .arg(Arg::new("Blocksize")
                               .short('b')
                               .long("blocksize")
                               .ignore_case(true)
                               .takes_value(true)
                               .max_values(1)
                               .possible_values(&["B", "K", "M", "G"])
                               .help("Sets the display blocksize to Bytes, Kilobytes, Megabytes or Gigabytes. Default is Kilobytes."))
                        .arg(Arg::new("Verbosity")
                                .short('v')
                                .long("verbosity")
                                .possible_values(&["quiet", "duplicates", "all"])
                                .ignore_case(true)
                                .takes_value(true)
                                .help("Sets verbosity for printed output."))
                        .arg(Arg::new("Output")
                                .short('o')
                                .long("output")
                                .takes_value(true)
                                .max_values(1)
                                .help("Sets file to save all output. Use 'no' for no file output."))
                        .arg(Arg::new("Format")
                                .short('f')
                                .long("format")
                                .possible_values(&["standard", "json"])
                                .takes_value(true)
                                .max_values(1)
                                .help("Sets output format."))
                        .arg(Arg::new("Minimum")
                                .short('m')
                                .long("minimum")
                                .takes_value(true)
                                .max_values(1)
                                .default_value("0")
                                .validator(|s| s.parse::<u64>())
                                .help("Minimum file size in bytes to consider."))
                        .get_matches();

    let search_dirs: Vec<_> = match arguments.values_of("directories") {
        Some(dirs) => dirs.collect(),
        None => vec![],
    };
    let ignore_dirs: Vec<_> = match arguments.values_of("ignore") {
        Some(dirs) => dirs.collect(),
        None => vec![],
    };
    let min_size: u64 = match arguments.value_of("Minimum") {
        Some(i) => i.parse::<u64>().unwrap_or(0),
        None => 0,
    };

    let (complete_files, read_errors): (Vec<Fileinfo>, Vec<(_, _)>) =
        ddh::deduplicate_dirs(search_dirs, ignore_dirs, min_size);
    let (shared_files, unique_files): (Vec<&Fileinfo>, Vec<&Fileinfo>) = complete_files
        .par_iter()
        .partition(|&x| x.get_paths().len() > 1);
    process_full_output(
        &shared_files,
        &unique_files,
        &complete_files,
        &read_errors,
        &arguments,
    );
}

fn process_full_output(
    shared_files: &[&Fileinfo],
    unique_files: &[&Fileinfo],
    complete_files: &[Fileinfo],
    error_paths: &[(PathBuf, std::io::Error)],
    arguments: &clap::ArgMatches,
) {
    let blocksize = match arguments.value_of("Blocksize").unwrap_or("") {
        "B" => "Bytes",
        "K" => "Kilobytes",
        "M" => "Megabytes",
        "G" => "Gigabytes",
        _ => "Megabytes",
    };
    let display_power = match blocksize {
        "Bytes" => 0,
        "Kilobytes" => 1,
        "Megabytes" => 2,
        "Gigabytes" => 3,
        _ => 2,
    };
    let display_divisor = 1024u64.pow(display_power);
    let fmt = match arguments.value_of("Format").unwrap_or("") {
        "standard" => PrintFmt::Standard,
        "json" => PrintFmt::Json,
        _ => PrintFmt::Standard,
    };
    let verbosity = match arguments.value_of("Verbosity").unwrap_or("") {
        "quiet" => Verbosity::Quiet,
        "duplicates" => Verbosity::Duplicates,
        "all" => Verbosity::All,
        _ => Verbosity::Quiet,
    };

    println!(
        "{} Total files (with duplicates): {} {}",
        complete_files
            .par_iter()
            .map(|x| x.get_paths().len() as u64)
            .sum::<u64>(),
        complete_files
            .par_iter()
            .map(|x| (x.get_paths().len() as u64) * x.get_length())
            .sum::<u64>()
            / (display_divisor),
        blocksize
    );
    println!(
        "{} Total files (without duplicates): {} {}",
        complete_files.len(),
        complete_files
            .par_iter()
            .map(|x| x.get_length())
            .sum::<u64>()
            / (display_divisor),
        blocksize
    );
    println!(
        "{} Single instance files: {} {}",
        unique_files.len(),
        unique_files.par_iter().map(|x| x.get_length()).sum::<u64>() / (display_divisor),
        blocksize
    );
    println!(
        "{} Shared instance files: {} {} ({} instances)",
        shared_files.len(),
        shared_files.par_iter().map(|x| x.get_length()).sum::<u64>() / (display_divisor),
        blocksize,
        shared_files
            .par_iter()
            .map(|x| x.get_paths().len() as u64)
            .sum::<u64>()
    );

    match (fmt, verbosity) {
        (_, Verbosity::Quiet) => {}
        (PrintFmt::Standard, Verbosity::Duplicates) => {
            println!("Shared instance files and instance locations");
            shared_files.iter().for_each(|x| {
                println!(
                    "instances of {} with file length {}:",
                    x.get_candidate_name(),
                    x.get_length()
                );
                x.get_paths()
                    .par_iter()
                    .for_each(|y| println!("\t{}", y.canonicalize().unwrap().to_str().unwrap()));
            })
        }
        (PrintFmt::Standard, Verbosity::All) => {
            println!("Single instance files");
            unique_files.par_iter().for_each(|x| {
                println!(
                    "{}",
                    x.get_paths()
                        .iter()
                        .next()
                        .unwrap()
                        .canonicalize()
                        .unwrap()
                        .to_str()
                        .unwrap()
                )
            });
            println!("Shared instance files and instance locations");
            shared_files.iter().for_each(|x| {
                println!(
                    "instances of {} with file length {}:",
                    x.get_candidate_name(),
                    x.get_length()
                );
                x.get_paths()
                    .par_iter()
                    .for_each(|y| println!("\t{}", y.canonicalize().unwrap().to_str().unwrap()));
            });
            error_paths.iter().for_each(|x| {
                println!(
                    "Could not process {:#?} due to error {:#?}",
                    x.0,
                    x.1.kind()
                );
            })
        }
        (PrintFmt::Json, Verbosity::Duplicates) => {
            println!(
                "{}",
                serde_json::to_string(shared_files).unwrap_or_else(|_| "".to_string())
            );
        }
        (PrintFmt::Json, Verbosity::All) => {
            println!(
                "{}",
                serde_json::to_string(complete_files).unwrap_or_else(|_| "".to_string())
            );
        }
    }

    match arguments.value_of("Output").unwrap_or("Results.txt") {
        "no" => {}
        destination_string => {
            match fs::File::open(destination_string) {
                Ok(_f) => {
                    println!("---");
                    println!("File {} already exists.", destination_string);
                    println!("Overwrite? Y/N");
                    let mut input = String::new();
                    match stdin().read_line(&mut input) {
                        Ok(_n) => match input.chars().next().unwrap_or(' ') {
                            'n' | 'N' => {
                                println!("Exiting.");
                                return;
                            }
                            'y' | 'Y' => {
                                println!("Over writing {}", destination_string);
                            }
                            _ => {
                                println!("Exiting.");
                                return;
                            }
                        },
                        Err(_e) => {
                            println!("Error encountered reading user input. Err: {}", _e);
                        }
                    }
                }
                Err(_e) => match fs::File::create(destination_string) {
                    Ok(_f) => {}
                    Err(_e) => {
                        println!(
                            "Error encountered opening file {}. Err: {}",
                            destination_string, _e
                        );
                        println!("Exiting.");
                        return;
                    }
                },
            }
            write_results_to_file(
                fmt,
                shared_files,
                unique_files,
                complete_files,
                destination_string,
            );
        }
    }
}

fn write_results_to_file(
    fmt: PrintFmt,
    shared_files: &[&Fileinfo],
    unique_files: &[&Fileinfo],
    complete_files: &[Fileinfo],
    file: &str,
) {
    let mut output = fs::File::create(file).expect("Error opening output file for writing");
    match fmt {
        PrintFmt::Standard => {
            output.write_fmt(format_args!("Duplicates:\n")).unwrap();
            for file in shared_files.iter() {
                let title = file.get_candidate_name();
                output.write_fmt(format_args!("{}\n", title)).unwrap();
                for entry in file.get_paths().iter() {
                    output
                        .write_fmt(format_args!("\t{}\n", entry.as_path().to_str().unwrap()))
                        .unwrap();
                }
            }
            output.write_fmt(format_args!("Singletons:\n")).unwrap();
            for file in unique_files.iter() {
                let title = file.get_candidate_name();
                output.write_fmt(format_args!("{}\n", title)).unwrap();
                for entry in file.get_paths().iter() {
                    output
                        .write_fmt(format_args!("\t{}\n", entry.as_path().to_str().unwrap()))
                        .unwrap();
                }
            }
        }
        PrintFmt::Json => {
            output
                .write_fmt(format_args!(
                    "{}",
                    serde_json::to_string(complete_files)
                        .unwrap_or_else(|_| "Error deserializing".to_string())
                ))
                .unwrap();
        }
    }
    println!("{:#?} results written to {}", fmt, file);
}
