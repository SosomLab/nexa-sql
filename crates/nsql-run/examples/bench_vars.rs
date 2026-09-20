//! 변수 관리 경로의 비용 계측(docs/63 §4 · V7 전수 점검) — `cargo run --release -p nsql-run --example bench_vars`.
//!
//! 재는 것: ① 분할 ② 실행 전 훑기(`missing_inputs` · D-137) ③ 문장 준비(`Engine::plan` = 치환 + 바인드 추출 + 방언 재작성)
//! ④ 변수 표 연산(대입·조회·바뀜 걷기) ⑤ 변수 사건의 복사 비용(탭 층 왕복). DB 왕복은 포함하지 않는다(순수 클라이언트 비용).

use nsql_core::{Dialect, Value};
use nsql_script::{missing_inputs, split_script_in, Engine, VarStore};
use std::collections::BTreeMap;
use std::time::Instant;

fn script(statements: usize) -> String {
    let mut s = String::with_capacity(statements * 110);
    s.push_str("EXEC :V_CD := 'SEBANG'\nEXEC :V_SEQ := 1\n");
    for i in 0..statements {
        s.push_str(&format!(
            "SELECT a.col_{i}, b.name, '&tag' AS tag FROM big_table_{i} a JOIN other b ON b.id = a.id WHERE a.cd = :V_CD AND a.seq > :V_SEQ + {i} -- note {i}\n;\n"
        ));
    }
    s
}

fn main() {
    for &n in &[1_000usize, 10_000, 40_000] {
        let src = script(n);
        let mb = src.len() as f64 / 1_048_576.0;
        let t = Instant::now();
        let items = split_script_in(&src, Some(Dialect::Oracle));
        let split = t.elapsed();

        let mut vars = VarStore::new();
        vars.assign("V_CD", Value::Str("SEBANG".into()));
        let mut defines = BTreeMap::new();
        defines.insert("TAG".to_string(), "x".to_string());
        let t = Instant::now();
        let needs = missing_inputs(&src, Some(Dialect::Oracle), &vars, &defines, Some('&'));
        let scan = t.elapsed();

        let mut e = Engine::new(Dialect::Oracle);
        e.define("TAG", "x");
        let t = Instant::now();
        let mut actions = 0usize;
        for it in &items {
            actions += e.plan(it).len();
        }
        let plan = t.elapsed();
        println!(
            "{n:>6} stmts · {mb:>5.2} MB | split {:>8.2} ms | pre-scan {:>8.2} ms ({} needs) | plan {:>8.2} ms = {:>6.1} µs/stmt | actions {actions}",
            split.as_secs_f64() * 1e3,
            scan.as_secs_f64() * 1e3,
            needs.len(),
            plan.as_secs_f64() * 1e3,
            plan.as_secs_f64() * 1e6 / items.len() as f64,
        );
    }
    // 바인드·치환 글자가 없는 큰 스크립트(덤프) = 빠른 길.
    let dump: String = (0..40_000)
        .map(|i| format!("INSERT INTO t VALUES ({i}, 'row {i}', 3.14);\n"))
        .collect();
    let t = Instant::now();
    let n = missing_inputs(
        &dump,
        Some(Dialect::Oracle),
        &VarStore::new(),
        &BTreeMap::new(),
        Some('&'),
    )
    .len();
    println!(
        "dump 40,000 stmts · {:.2} MB | pre-scan {:>8.3} ms ({n} needs)",
        dump.len() as f64 / 1_048_576.0,
        t.elapsed().as_secs_f64() * 1e3
    );
    // 변수 표: 1,000개 변수에서 대입·조회·상태 복사.
    let mut s = VarStore::new();
    let t = Instant::now();
    for i in 0..1_000 {
        s.assign(&format!("V_{i}"), Value::Str(format!("value-{i}")));
    }
    let assign = t.elapsed();
    let t = Instant::now();
    let mut hits = 0;
    for r in 0..100 {
        for i in 0..1_000 {
            hits += usize::from(s.get(&format!("v_{}", (i + r) % 1_000)).is_some());
        }
    }
    let get = t.elapsed();
    let t = Instant::now();
    let snap = s.local_states();
    let dirty = s.take_dirty();
    let copy = t.elapsed();
    println!(
        "vars 1,000 | assign {:>6.1} µs/op | get {:>6.3} µs/op ({hits}) | snapshot+dirty {:>6.2} ms ({} states · {} dirty)",
        assign.as_secs_f64() * 1e6 / 1_000.0,
        get.as_secs_f64() * 1e6 / 100_000.0,
        copy.as_secs_f64() * 1e3,
        snap.len(),
        dirty.len()
    );
    // 큰 값(1 MB 글) 하나의 탭 층 왕복(호스트 → 러너 → 호스트 = 복사 2회).
    let mut s = VarStore::new();
    s.assign("BIG", Value::Str("x".repeat(1 << 20)));
    let t = Instant::now();
    let a = s.local_states();
    let mut s2 = VarStore::new();
    s2.set_local(a);
    let b = s2.local_states();
    println!(
        "1 MB value round trip (2 copies) {:>6.2} ms ({} states)",
        t.elapsed().as_secs_f64() * 1e3,
        b.len()
    );
}
