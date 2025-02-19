#[allow(unused)]
pub enum Flags {
    ADInt,
    AIntDRational,
    ARationalDInterger,
    ARationalDRational,
}

// 00 - a1, d both integers
// 01 a1 interger, d rational
// 10 a1 rational, d interger
// 11 a1 rational, d rational
pub const fn arithmetic_sum(n: isize, a1: isize, d: isize, flags: Flags) -> isize {
    match flags {
        Flags::ADInt => (n / 2) * (2 * a1 + ((n - 1) * d)),
        Flags::AIntDRational => (n / 2) * (2 * a1 + ((n - 1) / d)),
        Flags::ARationalDInterger => (n / 2) * (2 / a1 + ((n - 1) * d)),
        Flags::ARationalDRational => (n / 2) * (2 / a1 + ((n - 1) / d)),
    }
}
