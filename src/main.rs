use crate::pattern::Pattern;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod pattern;
mod process;

#[tokio::main]
async fn main() {
    // skip name of executable
    let mut args = std::env::args().skip(1);

    // get pattern in first argument
    let Some(pattern) = args.next() else { return };
    let pattern = Box::leak(Box::new(Pattern::new(pattern.leak()))) as &'static Pattern<'static>;

    // collect all the args into a vector for repeated use
    let args = args.collect::<Vec<_>>();

    // initialize total counter in an atomic shared heap allocated number
    let total = Arc::new(AtomicUsize::new(0));

    // get time of start of enumeration
    let enumeration_start = Instant::now();

    // count all the paths to total
    process::process_paths(&args, {
        let total = total.clone();
        move |file| {
            if file.path().ends_with(".xml") {
                total.fetch_add(1, Ordering::SeqCst);
            }
            async {}
        }
    })
    .await;

    // read total into regular number
    let total = total.load(Ordering::SeqCst);

    // get time of start of processing
    let process_start = Instant::now();

    // initialize the channel to deliver completed work to the output printer
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<(String, String)>(64);

    // create the task that will process the files
    let file_processor = process::process_paths(args, move |file| {
        let sender = sender.clone();
        let path = file.path().to_owned();
        if path.ends_with(".xml") {
            match file.read_to_string() {
                Ok(xml) => {
                    // spawn tasks to do the heavy lifting
                    // these tasks prevent the main function from returning because they hold senders
                    // which are being reveived by the output_printer task which is joined on the main function
                    tokio::task::spawn_blocking(move || {
                        let output = parse_file(&xml, pattern);
                        let _ = futures::executor::block_on(sender.send((path, output)));
                    });
                }
                Err(()) => {}
            }
        }
        async {}
    });

    // create the task that will print the output and status to stdout and stderr
    let output_printer = async move {
        let mut count = 0;
        let mut stdout_lock = std::io::stdout().lock();
        let mut last_info = Instant::now();
        while let Some((path, output)) = receiver.recv().await {
            count += 1;
            if count % 101 == 0 || count == total {
                let now = Instant::now();
                if count == total || now - last_info > Duration::from_millis(500) {
                    last_info = now;
                    let elapsed_time = now - process_start;
                    let avg_time_per_iteration = elapsed_time / count as u32;
    
                    let seconds_per_iteration = avg_time_per_iteration.as_secs_f64();
                    let iterations_per_second = if seconds_per_iteration == 0.0 {
                        0.0
                    } else {
                        1.0 / seconds_per_iteration
                    };
    
                    let remaining_iterations = total - count;
                    let estimated_remaining_time = avg_time_per_iteration * remaining_iterations as u32;
    
                    eprintln!(
                        "({} / {}) {}% - ELAPSED: {:.2?} - FPS: {:.0?} - ERT: {:.2?} {}",
                        count,
                        total,
                        (count * 100) / total,
                        elapsed_time,
                        iterations_per_second,
                        estimated_remaining_time,
                        &path,
                    );
                }
            }
            stdout_lock.write_all(output.as_bytes()).expect("stdout failure");
        }
    };

    // execute both in paralel, wait for them to complete
    tokio::join!(output_printer, file_processor);

    let finished = Instant::now();

    // print times and quit
    eprintln!("ENUMERATED - {:#?}", process_start - enumeration_start);
    eprintln!("PROCESSED  - {:#?}", finished - process_start);
    eprintln!("TOTAL      - {:#?}", finished - enumeration_start);
}

fn parse_file(xml: &str, pattern: &Pattern) -> String {
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let table = print_by_pattern(&pattern, doc.root());
    let mut output = String::with_capacity(512);
    const SEPARATOR: &str = "|";
    for line in &table {
        if !line.is_empty() {
            output.push_str(line[0]);
            for cell in &line[1..] {
                output.push_str(SEPARATOR);
                output.push_str(cell);
            }
        }
        output.push('\n');
    }
    output
}

fn print_by_pattern<'a>(pattern: &Pattern<'a>, node: roxmltree::Node<'a, '_>) -> Vec<Vec<&'a str>> {
    if pattern.is_leaf() {
        return vec![vec![node.text().unwrap_or_default()]];
    }
    cartesian_product(
        &pattern
            .iter()
            .map(|(segment, sub_pattern)| {
                let table = node
                    .children()
                    .filter(|sub_node| Pattern::pattern_check(*segment, sub_node.tag_name().name()))
                    .map(|sub_node| print_by_pattern(sub_pattern, sub_node))
                    .flatten()
                    .collect::<Vec<_>>();
                if table.is_empty() {
                    return vec![vec![""; sub_pattern.count_leafs()]];
                }
                table
            })
            .collect::<Vec<_>>(),
    )
}

/*
[
    [
        [1, 2, 3]
        [100, 200, 300]
    ]
    [
        [0, 0]
    ]
    [
        [991]
        [992]
        [993]
    ]
] => [
    [1, 2, 3, 0, 0, 991]
    [1, 2, 3, 0, 0, 992]
    [1, 2, 3, 0, 0, 993]
    [100, 200, 300, 0, 0, 991]
    [100, 200, 300, 0, 0, 992]
    [100, 200, 300, 0, 0, 993]
]
*/
fn cartesian_product<T: Clone + std::fmt::Debug>(tables: &Vec<Vec<Vec<T>>>) -> Vec<Vec<T>> {
    if tables.is_empty() {
        return Vec::new();
    }
    if tables.iter().any(|x| x.is_empty()) {
        return vec![Vec::new()];
    }
    if let [table] = &tables[..] {
        return table.clone();
    }
    let mut indexes = vec![0usize; tables.len()];
    let mut table = Vec::new();
    loop {
        table.push(
            tables
                .iter()
                .enumerate()
                .map(|(index, table)| &table[indexes[index]])
                .flatten()
                .cloned()
                .collect(),
        );
        let mut i = indexes.len();
        loop {
            if i == 0 {
                return table;
            }
            i -= 1;
            indexes[i] += 1;
            if indexes[i] == tables[i].len() {
                indexes[i] = 0;
            } else {
                break;
            }
        }
    }
}
