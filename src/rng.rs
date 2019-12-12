// Copyright 2019 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under The General Public License (GPL), version 3.
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied. Please review the Licences for the specific language governing
// permissions and limitations relating to use of the SAFE Network Software.

pub use self::implementation::{new, new_from, MainRng};
#[cfg(any(feature = "mock_base"))]
pub use self::seed_printer::SeedPrinter;
#[cfg(any(test, feature = "mock_base"))]
pub use self::test::Seed;

use rand::Rng;

// `CryptoRng` trait shim.
// TODO: remove this when we update rand to more recent version as it has its own `CryptoRng` trait.
pub(crate) trait CryptoRng: Rng {}
impl<'a, R: CryptoRng> CryptoRng for &'a mut R {}
impl CryptoRng for MainRng {}

// Note: routing uses different version of the rand crate than threshold_crypto. This is a
// compatibility adapter between the two.
pub(crate) struct RngCompat<R>(pub R);

impl<R: Rng> rand_crypto::RngCore for RngCompat<R> {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_crypto::Error> {
        self.0.fill_bytes(dest);
        Ok(())
    }
}

impl<R: CryptoRng> rand_crypto::CryptoRng for RngCompat<R> {}

// Rng implementation used in production. Uses `OsRng` for maximum cryptographic security.
#[cfg(not(any(test, feature = "mock_base")))]
mod implementation {
    pub use rand::OsRng as MainRng;
    use rand::Rng;

    /// Create new rng instance.
    pub fn new() -> MainRng {
        match MainRng::new() {
            Ok(rng) => rng,
            Err(error) => panic!("Failed to create OsRng: {:?}", error),
        }
    }

    /// Same as `new`. Provided only for API parity with test/mock.
    pub fn new_from<R: Rng>(_: &mut R) -> MainRng {
        new()
    }
}

// Rng implementation used in tests. Uses `TestRng` to allow reproducible test results and
// to avoid opening too many file handles which could happen on some platforms if we used `OsRng`.
#[cfg(any(test, feature = "mock_base"))]
mod implementation {
    pub use super::test::TestRng as MainRng;
    use rand::{Rng, SeedableRng};

    /// Create new default rng instance.
    pub fn new() -> MainRng {
        MainRng::new()
    }

    /// Create new rng instance initialized with state generated by the provided rng.
    pub fn new_from<R: Rng>(rng: &mut R) -> MainRng {
        MainRng::from_seed(rng.gen())
    }
}

#[cfg(any(test, feature = "mock_base"))]
mod test {
    use rand::{Rand, Rng, SeedableRng, XorShiftRng};
    use std::{
        env,
        fmt::{self, Display, Formatter},
        str::FromStr,
    };
    use unwrap::unwrap;

    pub const SEED_ENV_NAME: &str = "SEED";

    /// Random number generator for tests that can be seeded using environment variable.
    /// Example: `SEED="[1, 2, 3, 4]"`
    pub struct TestRng(XorShiftRng);

    impl TestRng {
        /// Create new rng with default seed. That is, try to use the seed from environment variable
        /// if provided, otherwise use random seed.
        pub fn new() -> Self {
            Self::from_seed(Seed::default())
        }
    }

    impl Default for TestRng {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Rng for TestRng {
        fn next_u32(&mut self) -> u32 {
            self.0.next_u32()
        }

        fn next_u64(&mut self) -> u64 {
            self.0.next_u64()
        }

        fn fill_bytes(&mut self, dest: &mut [u8]) {
            self.0.fill_bytes(dest)
        }
    }

    impl SeedableRng<Seed> for TestRng {
        fn from_seed(seed: Seed) -> Self {
            Self(XorShiftRng::from_seed(seed.0))
        }

        fn reseed(&mut self, seed: Seed) {
            self.0.reseed(seed.0);
        }
    }

    impl rand_crypto::RngCore for TestRng {
        fn next_u32(&mut self) -> u32 {
            self.0.next_u32()
        }

        fn next_u64(&mut self) -> u64 {
            self.0.next_u64()
        }

        fn fill_bytes(&mut self, dest: &mut [u8]) {
            self.0.fill_bytes(dest)
        }

        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_crypto::Error> {
            self.0.fill_bytes(dest);
            Ok(())
        }
    }

    /// Seed for random number generators.
    #[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
    pub struct Seed([u32; 4]);

    impl Seed {
        /// Try to create seed by parsing the "SEED" env variable.
        ///
        /// # Panics
        ///
        /// Panics if the env variable is not empty but invalid.
        pub fn from_env() -> Option<Self> {
            if let Ok(value) = env::var(SEED_ENV_NAME) {
                Some(unwrap!(value.parse()))
            } else {
                None
            }
        }

        /// Create random seed.
        pub fn random() -> Self {
            Self(rand::thread_rng().gen())
        }
    }

    impl Default for Seed {
        fn default() -> Self {
            Self::from_env().unwrap_or_else(Self::random)
        }
    }

    impl FromStr for Seed {
        type Err = ParseError;

        fn from_str(mut input: &str) -> Result<Self, Self::Err> {
            let mut seed = [0u32; 4];

            skip_whitespace(&mut input);
            skip(&mut input, '[')?;

            for (index, value) in seed.iter_mut().enumerate() {
                skip_whitespace(&mut input);

                if index > 0 {
                    skip(&mut input, ',')?;
                    skip_whitespace(&mut input);
                }

                *value = parse_u32(&mut input)?;
            }

            skip_whitespace(&mut input);
            skip(&mut input, ']')?;

            Ok(Self(seed))
        }
    }

    impl Rand for Seed {
        fn rand<R: Rng>(rng: &mut R) -> Self {
            // Note: the `wrapping_add` trick is a workaround for what seems to be a weakness in
            // `XorShiftRng`. Without it we would sometimes end up with multiple rngs producing
            // identical values.
            // The idea is taken from: https://github.com/maidsafe/maidsafe_utilities/blob/24dfcbc6ee07a14bf64f3bc573f68cea01e06862/src/seeded_rng.rs#L92
            Self([
                rng.next_u32().wrapping_add(rng.next_u32()),
                rng.next_u32().wrapping_add(rng.next_u32()),
                rng.next_u32().wrapping_add(rng.next_u32()),
                rng.next_u32().wrapping_add(rng.next_u32()),
            ])
        }
    }

    impl Display for Seed {
        fn fmt(&self, f: &mut Formatter) -> fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    pub struct ParseError;

    fn skip_whitespace(input: &mut &str) {
        *input = input.trim_start();
    }

    fn skip(input: &mut &str, ch: char) -> Result<(), ParseError> {
        if input.starts_with(ch) {
            *input = &input[1..];
            Ok(())
        } else {
            Err(ParseError)
        }
    }

    fn parse_u32(input: &mut &str) -> Result<u32, ParseError> {
        let mut empty = true;
        let mut output = 0;

        while let Some(digit) = input.chars().next().and_then(|ch| ch.to_digit(10)) {
            empty = false;
            output = output * 10 + digit;
            *input = &input[1..];
        }

        if empty {
            Err(ParseError)
        } else {
            Ok(output)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parse_seed() {
            assert_eq!("[0, 0, 0, 0]".parse(), Ok(Seed([0, 0, 0, 0])));
            assert_eq!("[0, 1, 2, 3]".parse(), Ok(Seed([0, 1, 2, 3])));
            assert_eq!(
                "[2173726344, 4077344496, 2175816672, 3385125285]".parse(),
                Ok(Seed([
                    2_173_726_344,
                    4_077_344_496,
                    2_175_816_672,
                    3_385_125_285
                ]))
            );
            assert_eq!("".parse(), Err::<Seed, _>(ParseError));
        }
    }
}

#[cfg(feature = "mock_base")]
mod seed_printer {
    use super::Seed;
    use std::thread;

    /// Helper struct that prints a seed on scope exit.
    pub struct SeedPrinter {
        seed: Seed,
        mode: Mode,
    }

    #[derive(Eq, PartialEq)]
    enum Mode {
        OnFailure,
        OnSuccess,
    }

    impl SeedPrinter {
        /// Create new `SeedPrinter` that will print the given seed on failure.
        pub fn on_failure(seed: Seed) -> Self {
            interrupt::activate(seed);

            Self {
                seed,
                mode: Mode::OnFailure,
            }
        }

        /// Create new `SeedPrinter` that will print the given seed on success (exiting the scope
        /// without panic).
        pub fn on_success(seed: Seed) -> Self {
            Self {
                seed,
                mode: Mode::OnSuccess,
            }
        }

        /// Returns the seed this printer will print.
        pub fn seed(&self) -> &Seed {
            &self.seed
        }
    }

    impl Drop for SeedPrinter {
        fn drop(&mut self) {
            interrupt::deactivate();

            if thread::panicking() {
                if self.mode == Mode::OnFailure {
                    print_seed(&self.seed, "");
                }
            } else if self.mode == Mode::OnSuccess {
                print_seed(&self.seed, "");
            }
        }
    }

    fn print_seed(seed: &Seed, label: &str) {
        let msg = if label.is_empty() {
            format!("{}", seed)
        } else {
            format!("{}: {}", label, seed)
        };
        let border = (0..msg.len()).map(|_| "=").collect::<String>();
        println!("\n{}\n{}\n{}\n", border, msg, border);
    }

    // Print the seed also on SIGINT and SIGTERM
    mod interrupt {
        use super::{print_seed, Seed};
        use lazy_static::lazy_static;
        use std::{
            collections::HashMap,
            process,
            sync::{Mutex, Once},
            thread::{self, ThreadId},
        };

        static HANDLER_SET: Once = Once::new();

        lazy_static! {
            static ref ACTIVE_SEEDS: Mutex<HashMap<ThreadId, (String, Seed)>> =
                Mutex::new(HashMap::new());
        }

        pub fn activate(seed: Seed) {
            let mut map = ACTIVE_SEEDS.lock().unwrap();
            let _ = map.insert(
                thread::current().id(),
                (thread::current().name().unwrap_or("???").to_owned(), seed),
            );

            HANDLER_SET.call_once(|| {
                let _ = ctrlc::set_handler(|| {
                    let map = ACTIVE_SEEDS.lock().unwrap();
                    for (name, seed) in map.values() {
                        print_seed(seed, name)
                    }

                    process::abort();
                });
            })
        }

        pub fn deactivate() {
            let mut map = ACTIVE_SEEDS.lock().unwrap();
            let _ = map.remove(&thread::current().id());
        }
    }
}
