#!/usr/bin/env python3
"""Keys and signing for custom.txt, the custom-client config read by src/common.rs.

custom.txt is base64(signature || json), an ed25519 signed message as produced by
libsodium crypto_sign; read_custom_client() verifies it with the public key KEY.

  sign.py keygen <secret-key-file>       new key pair, prints the public key for KEY
  sign.py pubkey <secret-key-file>       prints the public key of an existing key
  sign.py sign <secret-key-file> <config.json> <custom.txt>
"""

import base64
import json
import os
import sys

from nacl.signing import SigningKey


def load_key(path):
    with open(path, 'rb') as f:
        return SigningKey(base64.b64decode(f.read().strip()))


def pubkey(sk):
    return base64.b64encode(sk.verify_key.encode()).decode()


def main(argv):
    if len(argv) == 2 and argv[0] == 'keygen':
        if os.path.exists(argv[1]):
            sys.exit(f'{argv[1]} already exists, refusing to overwrite')
        sk = SigningKey.generate()
        with open(argv[1], 'wb') as f:
            f.write(base64.b64encode(sk.encode()))
        print(pubkey(sk))
    elif len(argv) == 2 and argv[0] == 'pubkey':
        print(pubkey(load_key(argv[1])))
    elif len(argv) == 4 and argv[0] == 'sign':
        sk = load_key(argv[1])
        with open(argv[2], encoding='utf-8-sig') as f:
            config = json.load(f)
        data = json.dumps(config, ensure_ascii=False, separators=(',', ':')).encode()
        with open(argv[3], 'w', encoding='ascii') as f:
            f.write(base64.b64encode(sk.sign(data)).decode())
        print(f'{argv[3]} signed with {pubkey(sk)}')
    else:
        sys.exit(__doc__)


if __name__ == '__main__':
    main(sys.argv[1:])
