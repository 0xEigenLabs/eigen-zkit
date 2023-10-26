use crate::bellman_ce::pairing::{bls12_381::Bls12, bn256::Bn256};
use algebraic::{PrimeField, PrimeFieldRepr};
use franklin_crypto::bellman::{
    bls12_381::{
        Fq2 as Fq2_bls12381, G1Affine as G1Affine_bls12381, G2Affine as G2Affine_bls12381,
    },
    bn256::{Fq2, G1Affine, G2Affine},
    groth16::{Proof, VerifyingKey},
    CurveAffine,
};
use num_bigint::BigUint;
use num_traits::Num;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
#[derive(Debug, Serialize, Deserialize)]
pub struct G1 {
    pub x: String,
    pub y: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct G2 {
    pub x: [String; 2],
    pub y: [String; 2],
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyingKeyFile {
    #[serde(rename = "protocol")]
    pub protocol: String,
    #[serde(rename = "curve")]
    pub curve: String,
    #[serde(rename = "vk_alpha_1")]
    pub alpha_g1: G1,
    #[serde(rename = "vk_beta_1")]
    pub beta_g1: G1,
    #[serde(rename = "vk_beta_2")]
    pub beta_g2: G2,
    #[serde(rename = "vk_gamma_2")]
    pub gamma_g2: G2,
    #[serde(rename = "vk_delta_1")]
    pub delta_g1: G1,
    #[serde(rename = "vk_delta_2")]
    pub delta_g2: G2,
    #[serde(rename = "IC")]
    pub ic: Vec<G1>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProofFile {
    #[serde(rename = "pi_a")]
    pub a: G1,
    #[serde(rename = "pi_b")]
    pub b: G2,
    #[serde(rename = "pi_c")]
    pub c: G1,
    #[serde(rename = "protocol")]
    pub protocol: String,
    #[serde(rename = "curve")]
    pub curve: String,
}

pub trait Parser: franklin_crypto::bellman::pairing::Engine {
    fn parse_g1(e: &Self::G1Affine) -> (String, String);
    fn parse_g2(e: &Self::G2Affine) -> (String, String, String, String);
    fn parse_g1_json(e: &Self::G1Affine) -> G1 {
        let parsed = Self::parse_g1(e);
        G1 {
            x: parsed.0,
            y: parsed.1,
        }
    }
    fn parse_g2_json(e: &Self::G2Affine) -> G2 {
        let parsed = Self::parse_g2(e);
        G2 {
            x: (parsed.0, parsed.1).into(),
            y: (parsed.2, parsed.3).into(),
        }
    }
    fn to_g1(x: &str, y: &str) -> Self::G1Affine;
    fn to_g2(x0: &str, x1: &str, y0: &str, y1: &str) -> Self::G2Affine;
}

pub fn render_scalar_to_hex<F: PrimeField>(el: &F) -> String {
    let mut buff = vec![];
    let repr = el.into_repr();
    repr.write_be(&mut buff).unwrap();

    format!("0x{}", hex::encode(buff))
}

pub fn render_hex_to_scalar<F: PrimeField>(value: &str) -> F {
    let value = BigUint::from_str_radix(&value[2..], 16)
        .unwrap()
        .to_str_radix(10);
    F::from_str(&value).unwrap()
}

impl Parser for Bn256 {
    fn parse_g1(e: &Self::G1Affine) -> (String, String) {
        let (x, y) = e.into_xy_unchecked();
        (render_scalar_to_hex(&x), render_scalar_to_hex(&y))
    }

    fn parse_g2(e: &Self::G2Affine) -> (String, String, String, String) {
        let (x, y) = e.into_xy_unchecked();
        (
            render_scalar_to_hex(&x.c0),
            render_scalar_to_hex(&x.c1),
            render_scalar_to_hex(&y.c0),
            render_scalar_to_hex(&y.c1),
        )
    }

    fn to_g1(x: &str, y: &str) -> Self::G1Affine {
        G1Affine::from_xy_unchecked(render_hex_to_scalar(x), render_hex_to_scalar(y))
    }

    fn to_g2(x0: &str, x1: &str, y0: &str, y1: &str) -> Self::G2Affine {
        let x = Fq2 {
            c0: render_hex_to_scalar(x0),
            c1: render_hex_to_scalar(x1),
        };
        let y = Fq2 {
            c0: render_hex_to_scalar(y0),
            c1: render_hex_to_scalar(y1),
        };
        G2Affine::from_xy_unchecked(x, y)
    }
}

impl Parser for Bls12 {
    fn parse_g1(e: &Self::G1Affine) -> (String, String) {
        let (x, y) = e.into_xy_unchecked();
        (render_scalar_to_hex(&x), render_scalar_to_hex(&y))
    }

    fn parse_g2(e: &Self::G2Affine) -> (String, String, String, String) {
        let (x, y) = e.into_xy_unchecked();
        (
            render_scalar_to_hex(&x.c0),
            render_scalar_to_hex(&x.c1),
            render_scalar_to_hex(&y.c0),
            render_scalar_to_hex(&y.c1),
        )
    }

    fn to_g1(x: &str, y: &str) -> Self::G1Affine {
        G1Affine_bls12381::from_xy_unchecked(render_hex_to_scalar(x), render_hex_to_scalar(y))
    }

    fn to_g2(x0: &str, x1: &str, y0: &str, y1: &str) -> Self::G2Affine {
        let x = Fq2_bls12381 {
            c0: render_hex_to_scalar(x0),
            c1: render_hex_to_scalar(x1),
        };
        let y = Fq2_bls12381 {
            c0: render_hex_to_scalar(y0),
            c1: render_hex_to_scalar(y1),
        };
        G2Affine_bls12381::from_xy_unchecked(x, y)
    }
}

pub fn serialize_vk<P: Parser>(vk: &VerifyingKey<P>, curve_type: &str) -> String {
    let verifying_key_file = VerifyingKeyFile {
        protocol: "groth16".to_string(),
        curve: curve_type.to_string(),
        alpha_g1: P::parse_g1_json(&vk.alpha_g1),
        beta_g1: P::parse_g1_json(&vk.beta_g1),
        beta_g2: P::parse_g2_json(&vk.beta_g2),
        gamma_g2: P::parse_g2_json(&vk.gamma_g2),
        delta_g1: P::parse_g1_json(&vk.delta_g1),
        delta_g2: P::parse_g2_json(&vk.delta_g2),
        ic: vk.ic.iter().map(P::parse_g1_json).collect::<Vec<_>>(),
    };

    to_string(&verifying_key_file).unwrap()
}

pub fn serialize_proof<P: Parser>(p: &Proof<P>, curve_type: &str) -> String {
    let proof_file = ProofFile {
        a: P::parse_g1_json(&p.a),
        b: P::parse_g2_json(&p.b),
        c: P::parse_g1_json(&p.c),
        protocol: "groth16".to_string(),
        curve: curve_type.to_string(),
    };

    to_string(&proof_file).unwrap()
}

pub fn serialize_input<T: PrimeField>(inputs: &[T]) -> String {
    format!(
        "[\"{}\"]",
        inputs
            .iter()
            .map(render_scalar_to_hex)
            .collect::<Vec<_>>()
            .join("\", \""),
    )
}

pub fn to_verification_key<P: Parser>(s: &str) -> VerifyingKey<P> {
    let vk_file: VerifyingKeyFile =
        serde_json::from_str(s).expect("Error during deserialization of the JSON data");

    let convert_g1 = |point: &G1| P::to_g1(&point.x, &point.y);
    let convert_g2 = |point: &G2| P::to_g2(&point.x[0], &point.x[1], &point.y[0], &point.y[1]);

    VerifyingKey {
        alpha_g1: convert_g1(&vk_file.alpha_g1),
        beta_g1: convert_g1(&vk_file.beta_g1),
        beta_g2: convert_g2(&vk_file.beta_g2),
        gamma_g2: convert_g2(&vk_file.gamma_g2),
        delta_g1: convert_g1(&vk_file.delta_g1),
        delta_g2: convert_g2(&vk_file.delta_g2),
        ic: vk_file.ic.iter().map(convert_g1).collect(),
    }
}

pub fn to_proof<P: Parser>(s: &str) -> Proof<P> {
    let proof: ProofFile =
        serde_json::from_str(s).expect("Error during deserialization of the JSON data");

    let convert_g1 = |point: &G1| P::to_g1(&point.x, &point.y);
    let convert_g2 = |point: &G2| P::to_g2(&point.x[0], &point.x[1], &point.y[0], &point.y[1]);

    Proof {
        a: convert_g1(&proof.a),
        b: convert_g2(&proof.b),
        c: convert_g1(&proof.c),
    }
}

pub fn to_public_input<T: PrimeField>(s: &str) -> Vec<T> {
    let input: Vec<String> = serde_json::from_str(s).unwrap();
    input
        .iter()
        .map(|hex_str| render_hex_to_scalar::<T>(hex_str))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bellman_ce::groth16::{Proof, VerifyingKey};
    use crate::bellman_ce::pairing::{
        bls12_381::Bls12,
        bn256::{Bn256, Fr},
    };
    use crate::bellman_ce::plonk::better_cs::keys::read_fr_vec;

    #[test]
    fn test_serialize_vk() {
        let mut reader = std::io::BufReader::with_capacity(
            1 << 24,
            std::fs::File::open("./test-vectors/verification_key.bin").unwrap(),
        );
        let vk_from_bin = VerifyingKey::<Bn256>::read(&mut reader).unwrap();
        let result = serialize_vk(&vk_from_bin, "bn128");
        std::fs::write("./test-vectors/verification_key.json", result)
            .expect("Unable to write data to file");

        let json_data = std::fs::read_to_string("./test-vectors/verification_key.json")
            .expect("Unable to read the JSON file");
        let verifying_key_from_json = to_verification_key::<Bn256>(&json_data);
        assert_eq!(
            vk_from_bin.alpha_g1, verifying_key_from_json.alpha_g1,
            "VerificationKey are not equal"
        );
    }

    #[test]
    fn test_serialize_vk_bls12381() {
        let mut reader = std::io::BufReader::with_capacity(
            1 << 24,
            std::fs::File::open("./test-vectors/verification_key_bls12381.bin").unwrap(),
        );
        let vk_from_bin = VerifyingKey::<Bls12>::read(&mut reader).unwrap();
        let result = serialize_vk(&vk_from_bin, "bls12381");
        std::fs::write("./test-vectors/verification_key_bls12381.json", result)
            .expect("Unable to write data to file");
        let json_data = std::fs::read_to_string("./test-vectors/verification_key_bls12381.json")
            .expect("Unable to read the JSON file");
        let verifying_key_from_json = to_verification_key::<Bls12>(&json_data);
        assert_eq!(
            vk_from_bin.alpha_g1, verifying_key_from_json.alpha_g1,
            "VerificationKey are not equal"
        );
    }

    #[test]
    fn test_serialize_proof() {
        let mut reader = std::io::BufReader::with_capacity(
            1 << 24,
            std::fs::File::open("./test-vectors/proof.bin").unwrap(),
        );
        let proof_from_bin = Proof::<Bn256>::read(&mut reader).unwrap();
        let result = serialize_proof(&proof_from_bin, "bn128");
        std::fs::write("./test-vectors/proof.json", result).expect("Unable to write data to file");

        let json_data = std::fs::read_to_string("./test-vectors/proof.json")
            .expect("Unable to read the JSON file");
        let proof_from_json = to_proof::<Bn256>(&json_data);
        assert_eq!(proof_from_bin.a, proof_from_json.a, "Proofs are not equal");
    }

    #[test]
    fn test_serialize_input() {
        let mut reader = std::io::BufReader::with_capacity(
            1 << 24,
            std::fs::File::open("./test-vectors/public_input.bin").unwrap(),
        );
        let input_from_bin = read_fr_vec::<Fr, _>(&mut reader).unwrap();
        let result = serialize_input::<Fr>(&input_from_bin);
        std::fs::write("./test-vectors/public_input.json", result)
            .expect("Unable to write data to file");

        let json_data = std::fs::read_to_string("./test-vectors/public_input.json")
            .expect("Unable to read the JSON file");
        let input_from_json = to_public_input::<Fr>(&json_data);
        assert_eq!(input_from_bin[0], input_from_json[0], "Input are not equal");
    }
}
