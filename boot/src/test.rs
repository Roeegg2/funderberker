pub fn test_runner(tests: &[&(fn(), &str)]) {
    logger::info!("Running {} tests", tests.len());

    for (i, test) in tests.iter().enumerate() {
        print!("(TEST) Test number {}: \"{}\" ... ", i + 1, test.1);
        (test.0)();
        println!("[OK]");
    }

    println!("(TEST) All tests passed!");
}
