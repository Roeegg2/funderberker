pub fn test_runner(tests: &[&dyn Fn()]) {
    println!("(TEST) Running {} tests", tests.len());

    for (i, test) in tests.iter().enumerate() {
        print!("(TEST) Test number: {}: ...", i);
        test();
        println!("(TEST) [OK]");
    }

    println!("(TEST) All tests passed!");
}
