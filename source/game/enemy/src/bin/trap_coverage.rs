//! Reads `table,index` pairs (one per line) and prints each with whether the
//! engine implements its trap, for sizing what a script needs.
use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let mut parts = line.split(',').map(|p| p.trim().parse::<u16>());
        let (Some(Ok(table)), Some(Ok(index))) = (parts.next(), parts.next()) else {
            continue;
        };
        let name = enemy::trap_table::lookup(table, index).map_or("?", |t| t.name);
        let state = if enemy::traps::lookup(table, index).is_some() {
            "implemented"
        } else {
            "missing"
        };
        println!("{table},{index} {name} {state}");
    }
}
