use fcsd::Set;
use sophia::graph::inmem::TermIndexMapU;
use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Weak;
use weak_table::WeakHashSet;

fn foo() {
    let mut whs: WeakHashSet<Weak<Vec<u8>>, RandomState> = WeakHashSet::new();
    let mut barvec: Vec<String> = Vec::new();
    for i in 1..10000000 {
        barvec.push("blablablablablabla".to_owned() + &i.to_string());
    }
    let bars: Set = Set::new(barvec).unwrap();
    for bar in bars.iter() {
        let x: Arc<Vec<u8>> = Arc::new(bar.1);
        whs.insert(x);
    }
    let mut tim: TermIndexMapU<u32, WeakHashSet<Weak<str>, RandomState>> = TermIndexMapU::new();
}

// HashGraph<TermIndexMapU<u32, WeakHashSet<Weak<str>, RandomState>>>;
/*
fn measure(g: &GraphType) {
 log::info!("blubb ");
}
*/
