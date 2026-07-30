# Stage 0 release network identities

These are public cryptographic identities. They are not seed values, private
keys, or mnemonics.

## Authorities

| Authority | Aura public key | GRANDPA public key |
| --- | --- | --- |
| 1 | `0x66d09cb4dff344d5a6b07ca9909dc05f46ccda6687c52d7dad9983c7fe891619` | `0x4e0ca8042d49c595cf51103a96313bcf72b37cc178b7615382e9358fd95bee0d` |

## Workers

For the minimal Stage 0 release, each Worker's owner account and registered
session key use the same sr25519 public key.

| Worker ID | Account/session public key |
| --- | --- |
| 0 | `0x326c5d73920e92464386ba7a3ac19522b855edc95088b2933fa5470fca941a0f` |
| 1 | `0xdcf28e115d439663a12e8cba10e76bc6e9503469ad27ab03e1140ddac931cc53` |
| 2 | `0xce5c3f3290a1ac97f3145d96c7a49d14a8b670f465a1de88d7c520cb6d11273d` |

The corresponding credentials exist only in the operator's ignored secret
directory and configured Actions secrets. They are never included in source
control, images, logs, or release artifacts.
