"""Encode a bounded SFOSBC09 transport. Kernel remains the opcode authority."""
import argparse
import json
import struct
from pathlib import Path


def integer(value, maximum):
    if type(value) is not int or not 0 <= value <= maximum:
        raise ValueError('Expected bounded unsigned integer')
    return value


def encode(spec):
    if type(spec) is not dict or set(spec) != {'programs'}:
        raise ValueError('Expected programs object')
    programs = spec['programs']
    if type(programs) is not list or not 1 <= len(programs) <= 4:
        raise ValueError('Expected 1..4 programs')
    output = bytearray(struct.pack('<8sHHI', b'SFOSBC09', 1, len(programs), 0))
    total_fuel = total_code = 0
    for program in programs:
        if type(program) is not dict or set(program) != {'fuel', 'instructions'}:
            raise ValueError('Expected fuel/instructions')
        fuel = integer(program['fuel'], 2097152)
        instructions = program['instructions']
        if fuel == 0 or type(instructions) is not list or not 1 <= len(instructions) <= 16:
            raise ValueError('Empty fuel or invalid instruction count')
        code = bytearray()
        for word in instructions:
            if type(word) is not list or len(word) != 5:
                raise ValueError('Instruction needs opcode,dst,src,aux,immediate')
            code.extend(struct.pack('<BBBBQI', *(integer(value, 255) for value in word[:4]), integer(word[4], 2**64-1), 0))
        total_code += len(code)
        total_fuel += fuel
        if total_code > 512 or total_fuel > 2097152:
            raise ValueError('Aggregate code/fuel quota exceeded')
        output.extend(struct.pack('<IIQ', len(code), fuel, 0))
        output.extend(code)
    return bytes(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    # Bound input before JSON parsing. Output is at most 592 bytes, no network.
    with args.input.open('rb') as source:
        data = source.read(65537)
    if len(data) > 65536:
        parser.error('Input JSON exceeds 64 KiB')
    payload = encode(json.loads(data))
    # Refuse accidental overwrites, including input == output.
    with args.output.open('xb') as destination:
        destination.write(payload)
    print(f'Wrote {len(payload)} bytes to {args.output}')


if __name__ == '__main__':
    main()
