pub fn test_runner(tests: &[&dyn Fn()]) {
    log_info!("Running {} tests", tests.len());

    for (i, test) in tests.iter().enumerate() {
        print!("(TEST) Test number: {}: ... ", i);
        test();
        println!("[OK]");
    }

    println!("(TEST) All tests passed!");
}
