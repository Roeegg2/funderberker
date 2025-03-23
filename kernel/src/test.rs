pub fn test_runner(tests: &[&dyn Fn()]) {
    log!("(TEST) Running {} tests", tests.len());

    for (i, test) in tests.iter().enumerate() {
        log!("(TEST) Test number: {}: ...", i);
        test();
        log!("(TEST) [OK]");
    }

    log!("(TEST) All tests passed!");
}
