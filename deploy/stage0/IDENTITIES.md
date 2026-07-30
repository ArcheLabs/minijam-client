# Stage 0 release network identities

These are public cryptographic identities. They are not seed values, private
keys, or mnemonics.

## Authorities

| Authority | Aura public key | GRANDPA public key |
| --- | --- | --- |
| 1 | `0x66d09cb4dff344d5a6b07ca9909dc05f46ccda6687c52d7dad9983c7fe891619` | `0x4e0ca8042d49c595cf51103a96313bcf72b37cc178b7615382e9358fd95bee0d` |
| 2 | `0x48b7466c50e5e7add7b1bd95a70f93c80cc93d398015356d2a0818f504cc444a` | `0x2fb01b7919651aafdfbd9c46ab6e5c425332756040ad65f3074a68e8dc08ebba` |
| 3 | `0x0eefa817f7b8152add2f6d9241df5a88bd6b760ef75a759352ae5fca70319306` | `0xaf25f435441122ba193bbffc4718638b015971cd169b78dbf0cdedf4a1556a82` |

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
