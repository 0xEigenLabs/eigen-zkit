const temp = require("temp");
const path = require("path");
const fs = require("fs");

const circom_wasm = require("circom_tester").wasm;

export async function genMain(template_file, template_name, publics = "", params = [], opt = {}, tester = circom_wasm) {
    temp.track();

    const temp_circuit = await temp.open({prefix: template_name, suffix: ".circom"});
    console.log("---",template_name)
    const include_path = path.relative(temp_circuit.path, template_file);
    const params_string = JSON.stringify(params).slice(1, -1);

    let main = "main";
    if (publics.length > 0) {
      main = `main { public [${publics}] }`
    }

    fs.writeSync(temp_circuit.fd, `
pragma circom 2.0.0;
include "${include_path}";
component ${main} = ${template_name} (${params_string});
    `);

    console.log(temp_circuit.path, opt)
    return tester(temp_circuit.path, opt);
}

export async function executeCircuit(
  circuit,
  inputs
) {
  const witness = await circuit.calculateWitness(inputs, true)
  await circuit.checkConstraints(witness)
  await circuit.loadSymbols()
  return witness
}
