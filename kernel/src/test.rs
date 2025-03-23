pub fn test_runner(tests: &[&dyn Fn()]) {
    println!("Running {} tests", tests.len());

    for (i, test) in tests.iter().enumerate() {
        print!("Test #{}: ", i);
        test();
        println!("[OK]");
    }

    println!("All tests passed!");
}
