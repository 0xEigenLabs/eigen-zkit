// copied and modified by https://github.com/arkworks-rs/circom-compat/blob/master/src/witness/witness_calculator.rs
use super::Circom;
use super::{fnv, CircomBase, SafeMemory, Wasm};
use crate::bellman_ce::{PrimeField, ScalarEngine};
use crate::errors::{EigenError, Result};
use num::ToPrimitive;
use num_bigint::BigInt;
use num_bigint::BigUint;
use num_bigint::Sign;
use num_traits::{One, Zero};
use serde_json::Value;
use std::str::FromStr;
use wasmer::{imports, Function, Instance, Memory, MemoryType, Module, Store};

#[cfg(not(feature = "wasm"))]
use std::fs::OpenOptions;

#[cfg(not(feature = "wasm"))]
use std::io::{BufWriter, Write};

use byteorder::{LittleEndian, WriteBytesExt};

#[derive(Clone, Debug)]
pub struct WitnessCalculator {
    pub instance: Wasm,
    pub memory: SafeMemory,
    pub n64: u32,
    pub circom_version: u32,
}

fn from_array32(arr: Vec<u32>) -> BigInt {
    let mut res = BigInt::zero();
    let radix = BigInt::from(0x100000000u64);
    for &val in arr.iter() {
        res = res * &radix + BigInt::from(val);
    }
    res
}

fn to_array32(s: &BigInt, size: usize) -> Vec<u32> {
    let mut res = vec![0; size];
    let mut rem = s.clone();
    let radix = BigInt::from(0x100000000u64);
    let mut c = size;
    while !rem.is_zero() {
        c -= 1;
        res[c] = (&rem % &radix).to_u32().unwrap();
        rem /= &radix;
    }

    res
}

impl WitnessCalculator {
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::from_file(path)
    }

    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let store = Store::default();
        let module = Module::from_file(&store, path).expect("correct wtns file");
        Self::from_module(module)
    }

    pub fn from_module(module: Module) -> Result<Self> {
        let store = module.store();

        // Set up the memory
        let memory = Memory::new(store, MemoryType::new(2000, None, false)).unwrap();
        let import_object = imports! {
            "env" => {
                "memory" => memory.clone(),
            },
            // Host function callbacks from the WASM
            "runtime" => {
                "error" => runtime::error(store),
                "logSetSignal" => runtime::log_signal(store),
                "logGetSignal" => runtime::log_signal(store),
                "logFinishComponent" => runtime::log_component(store),
                "logStartComponent" => runtime::log_component(store),
                "log" => runtime::log_component(store),
                "exceptionHandler" => runtime::exception_handler(store),
                "showSharedRWMemory" => runtime::show_memory(store),
                "printErrorMessage" => runtime::print_error_message(store),
                "writeBufferMessage" => runtime::write_buffer_message(store),
            }
        };
        let instance = Wasm::new(Instance::new(&module, &import_object)?);

        let version = instance.get_version().unwrap_or(1);

        // Circom 2 feature flag with version 2
        fn new_circom(instance: Wasm, memory: Memory, version: u32) -> Result<WitnessCalculator> {
            let n32 = instance.get_field_num_len32()?;
            let mut safe_memory = SafeMemory::new(memory, n32 as usize, BigInt::zero());
            instance.get_raw_prime()?;
            let mut arr = vec![0; n32 as usize];
            for i in 0..n32 {
                let res = instance.read_shared_rw_memory(i)?;
                arr[(n32 as usize) - (i as usize) - 1] = res;
            }
            let prime = from_array32(arr);

            let n64 = ((prime.bits() - 1) / 64 + 1) as u32;
            safe_memory.prime = prime;

            Ok(WitnessCalculator {
                instance,
                memory: safe_memory,
                n64,
                circom_version: version,
            })
        }

        new_circom(instance, memory, version)
    }

    pub fn calculate_witness<I: IntoIterator<Item = (String, Vec<BigInt>)>>(
        &mut self,
        inputs: I,
        sanity_check: bool,
    ) -> Result<Vec<BigInt>> {
        self.instance.init(sanity_check)?;
        let wtns_u32 = self.calculate_witness_circom(inputs, sanity_check)?;
        let n32 = self.instance.get_field_num_len32()?;

        let mut wo = Vec::new();
        let witness_size = self.instance.get_witness_size()?;
        for i in 0..witness_size {
            let mut arr = vec![0u32; n32 as usize];
            for j in 0..n32 {
                arr[(n32 - 1 - j) as usize] = wtns_u32[(i * n32 + j) as usize];
            }
            wo.push(from_array32(arr));
        }
        Ok(wo)
    }

    pub fn calculate_witness_bin<I: IntoIterator<Item = (String, Vec<BigInt>)>>(
        &mut self,
        inputs: I,
        sanity_check: bool,
    ) -> Result<Vec<u32>> {
        self.instance.init(sanity_check)?;
        self.calculate_witness_circom(inputs, sanity_check)
    }

    // Circom 2 feature flag with version 2
    fn calculate_witness_circom<I: IntoIterator<Item = (String, Vec<BigInt>)>>(
        &mut self,
        inputs: I,
        sanity_check: bool,
    ) -> Result<Vec<u32>> {
        self.instance.init(sanity_check)?;

        let n32 = self.instance.get_field_num_len32()?;

        // allocate the inputs
        for (name, values) in inputs.into_iter() {
            let (msb, lsb) = fnv(&name);

            for (i, value) in values.into_iter().enumerate() {
                let f_arr = to_array32(&value, n32 as usize);
                for j in 0..n32 {
                    self.instance
                        .write_shared_rw_memory(j, f_arr[(n32 as usize) - 1 - (j as usize)])?;
                }
                self.instance.set_input_signal(msb, lsb, i as u32)?;
            }
        }

        let mut w = Vec::new();

        let witness_size = self.instance.get_witness_size()?;
        for i in 0..witness_size {
            self.instance.get_witness(i)?;
            for j in 0..n32 {
                w.push(self.instance.read_shared_rw_memory(j)?);
            }
        }

        Ok(w)
    }

    #[cfg(not(feature = "wasm"))]
    pub fn save_witness_to_bin_file<E: ScalarEngine>(
        &self,
        filename: &str,
        w: &Vec<u32>,
    ) -> Result<()> {
        let writer = OpenOptions::new()
            .write(true)
            .create(true)
            .open(filename)
            .expect("unable to open.");

        let writer = BufWriter::new(writer);
        self.save_witness_from_bin_writer::<E, _>(writer, w)
    }

    pub fn save_witness_from_bin_writer<E: ScalarEngine, W: Write>(
        &self,
        mut writer: W,
        wtns: &Vec<u32>,
    ) -> Result<()> {
        let n32 = self.instance.get_field_num_len32()?;
        let wtns_header = [119, 116, 110, 115];
        writer.write_all(&wtns_header)?;

        let version = self.circom_version;
        writer.write_u32::<LittleEndian>(version)?;
        let num_section = 2u32;
        writer.write_u32::<LittleEndian>(num_section)?;

        // id section 1
        let id_section = 1u32;
        writer.write_u32::<LittleEndian>(id_section)?;

        let sec_size: u64 = (n32 * 4 + 8) as u64;
        writer.write_u64::<LittleEndian>(sec_size)?;

        let field_size: u32 = n32 * 4;
        writer.write_u32::<LittleEndian>(field_size)?;

        // write prime
        let (sign, prime_buf) = self.memory.prime.to_bytes_le();
        if sign != Sign::Plus {
            return Err(EigenError::Unknown(format!(
                "Invalid prime: {}, must be positive",
                self.memory.prime
            )));
        }
        if prime_buf.len() as u32 != field_size {
            return Err(EigenError::Unknown(format!(
                "Invalid prime: {}, len must be of {}",
                self.memory.prime,
                prime_buf.len()
            )));
        }
        writer.write_all(&prime_buf)?;

        // write witness size
        let wtns_size = wtns.len() as u32 / n32;
        writer.write_u32::<LittleEndian>(wtns_size)?;
        // sec type
        writer.write_u32::<LittleEndian>(2)?;
        // sec size
        writer.write_u64::<LittleEndian>((wtns_size * field_size) as u64)?;

        for i in 0..wtns.len() {
            writer.write_u32::<LittleEndian>(wtns[i])?;
        }
        Ok(())
    }

    pub fn calculate_witness_element<
        E: ScalarEngine,
        I: IntoIterator<Item = (String, Vec<BigInt>)>,
    >(
        &mut self,
        inputs: I,
        sanity_check: bool,
    ) -> Result<Vec<E::Fr>> {
        let witness = self.calculate_witness(inputs, sanity_check)?;
        let modulus = BigUint::from_str(
            "21888242871839275222246405745257275088548364400416034343698204186575808495617",
        )?;

        // convert it to field elements
        use num_traits::Signed;
        let witness = witness
            .into_iter()
            .map(|w| {
                let w = if w.sign() == num_bigint::Sign::Minus {
                    // Need to negate the witness element if negative
                    modulus.clone() - w.abs().to_biguint().unwrap()
                } else {
                    w.to_biguint().unwrap()
                };
                E::Fr::from_str(&w.to_string()).unwrap()
            })
            .collect::<Vec<_>>();

        Ok(witness)
    }
}

#[allow(dead_code)]
pub fn value_to_bigint(v: Value) -> BigInt {
    match v {
        Value::String(inner) => BigInt::from_str(&inner).unwrap(),
        Value::Number(inner) => BigInt::from(inner.as_u64().expect("not a u32")),
        _ => panic!("unsupported type {:?}", v),
    }
}

pub fn flat_array(v: &Vec<Value>) -> Vec<BigInt> {
    let mut result = Vec::new();
    fn fill_array(out: &mut Vec<BigInt>, value: &Value) {
        match value {
            Value::Array(inner) => {
                for v2 in inner.iter() {
                    fill_array(out, v2);
                }
            }
            Value::Bool(inner) => {
                if *inner {
                    out.push(BigInt::one());
                } else {
                    out.push(BigInt::zero());
                }
            }
            Value::String(inner) => {
                out.push(BigInt::from_str(inner).unwrap());
            }
            Value::Number(inner) => {
                out.push(BigInt::from_str(&inner.to_string()).unwrap());
            }
            _ => panic!(),
        }
    }

    for v2 in v.iter() {
        fill_array(&mut result, v2);
    }
    result
}

// callback hooks for debugging
mod runtime {
    use super::*;

    pub fn error(store: &Store) -> Function {
        #[allow(unused)]
        #[allow(clippy::many_single_char_names)]
        fn func(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32) -> Result<()> {
            // NOTE: We can also get more information why it is failing, see p2str etc here:
            // https://github.com/iden3/circom_runtime/blob/master/js/witness_calculator.js#L52-L64
            log::debug!("runtime error, exiting early: {a} {b} {c} {d} {e} {f}",);
            Err(EigenError::WasmerRuntimeError(wasmer::RuntimeError::new(
                "1",
            )))
        }
        Function::new_native(store, func)
    }

    // Circom 2.0
    pub fn exception_handler(store: &Store) -> Function {
        #[allow(unused)]
        fn func(a: i32) {}
        Function::new_native(store, func)
    }

    // Circom 2.0
    pub fn show_memory(store: &Store) -> Function {
        #[allow(unused)]
        fn func() {}
        Function::new_native(store, func)
    }

    // Circom 2.0
    pub fn print_error_message(store: &Store) -> Function {
        #[allow(unused)]
        fn func() {}
        Function::new_native(store, func)
    }

    // Circom 2.0
    pub fn write_buffer_message(store: &Store) -> Function {
        #[allow(unused)]
        fn func() {}
        Function::new_native(store, func)
    }

    pub fn log_signal(store: &Store) -> Function {
        #[allow(unused)]
        fn func(a: i32, b: i32) {}
        Function::new_native(store, func)
    }

    pub fn log_component(store: &Store) -> Function {
        #[allow(unused)]
        fn func(a: i32) {}
        Function::new_native(store, func)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, path::PathBuf};

    struct TestCase<'a> {
        circuit_path: &'a str,
        inputs_path: &'a str,
        n64: u32,
        witness: &'a [&'a str],
    }

    pub fn root_path(p: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push(p);
        path.to_string_lossy().to_string()
    }

    #[test]
    fn multiplier_1() {
        run_test(TestCase {
            circuit_path: root_path("test-vectors/mycircuit.wasm").as_str(),
            inputs_path: root_path("test-vectors/mycircuit-input1.json").as_str(),
            n64: 4,
            witness: &["1", "33", "3", "11"],
        });
    }

    #[test]
    fn multiplier_2() {
        run_test(TestCase {
            circuit_path: root_path("test-vectors/mycircuit.wasm").as_str(),
            inputs_path: root_path("test-vectors/mycircuit-input2.json").as_str(),
            n64: 4,
            witness: &[
                "1",
                "21888242871839275222246405745257275088548364400416034343698204186575672693159",
                "21888242871839275222246405745257275088548364400416034343698204186575796149939",
                "11",
            ],
        });
    }

    #[test]
    fn multiplier_3() {
        run_test(TestCase {
            circuit_path: root_path("test-vectors/mycircuit.wasm").as_str(),
            inputs_path: root_path("test-vectors/mycircuit-input3.json").as_str(),
            n64: 4,
            witness: &[
                "1",
                "21888242871839275222246405745257275088548364400416034343698204186575808493616",
                "10944121435919637611123202872628637544274182200208017171849102093287904246808",
                "2",
            ],
        });
    }

    // TODO: test complex samples

    fn run_test(case: TestCase) {
        let mut wtns = WitnessCalculator::new(case.circuit_path).unwrap();
        assert_eq!(
            wtns.memory.prime.to_str_radix(16),
            "30644E72E131A029B85045B68181585D2833E84879B9709143E1F593F0000001".to_lowercase()
        );
        assert_eq!({ wtns.n64 }, case.n64);

        let inputs_str = std::fs::read_to_string(case.inputs_path).unwrap();
        let inputs: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(&inputs_str).unwrap();

        let inputs = inputs
            .iter()
            .map(|(key, value)| {
                let res = match value {
                    Value::String(inner) => {
                        vec![BigInt::from_str(inner).unwrap()]
                    }
                    Value::Number(inner) => {
                        vec![BigInt::from(inner.as_u64().expect("not a u32"))]
                    }
                    Value::Array(inner) => inner.iter().cloned().map(value_to_bigint).collect(),
                    _ => panic!(),
                };

                (key.clone(), res)
            })
            .collect::<HashMap<_, _>>();

        let res = wtns.calculate_witness(inputs, true).unwrap();
        for (r, w) in res.iter().zip(case.witness) {
            assert_eq!(r, &BigInt::from_str(w).unwrap());
        }
    }
}
