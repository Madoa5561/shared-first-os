import copy
import struct
import unittest
from pack_bytecode import encode


class PackerTests(unittest.TestCase):
    def spec(self):
        return {'programs': [{'fuel': 2, 'instructions': [[1, 0, 0, 0, 42], [0, 0, 0, 0, 0]]}]}

    def test_layout_is_exact_little_endian(self):
        data = encode(self.spec())
        self.assertEqual(len(data), 64)
        self.assertEqual(struct.unpack('<8sHHI', data[:16]), (b'SFOSBC09', 1, 1, 0))
        self.assertEqual(struct.unpack('<IIQ', data[16:32]), (32, 2, 0))
        self.assertEqual(struct.unpack('<BBBBQI', data[32:48]), (1, 0, 0, 0, 42, 0))

    def test_rejects_bad_shapes_and_integers(self):
        for value in [None, [], {}, {'programs': []}, {'programs': [None]}, {'programs': [], 'extra': 1}]:
            with self.assertRaises(ValueError):
                encode(value)
        for value in [-1, True, 2**64, 1.5, '42']:
            spec = self.spec()
            spec['programs'][0]['instructions'][0][4] = value
            with self.assertRaises(ValueError):
                encode(spec)

    def test_code_fuel_and_task_quotas(self):
        for scenario in ['code', 'fuel', 'count', 'zero']:
            spec = self.spec()
            program = spec['programs'][0]
            if scenario == 'code':
                program['instructions'] *= 8
                spec['programs'] = [copy.deepcopy(program) for _ in range(3)]
            elif scenario == 'fuel':
                program['fuel'] = 2097152
                spec['programs'].append(copy.deepcopy(program))
            elif scenario == 'count':
                spec['programs'] *= 5
            else:
                program['fuel'] = 0
            with self.assertRaises(ValueError):
                encode(spec)
