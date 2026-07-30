# Stage 0 public accounts

The Stage 0 chain specification assigns the following public accounts. These
values are public `AccountId32` identifiers, not private keys, seeds, or
mnemonics.

| Role | Public key | SS58 address | Genesis balance |
| --- | --- | --- | --- |
| Faucet | `0x1a690444d160a1f63281203ede449ba996c560b7980e404375765f2aeacd886a` | `5CfLJGrEfAnDLbNGQuSa5CUwGgU13gt7rsWXJLCsNCMFjDUr` | 1,000,000 MINI |
| Sudo | `0x64da539020cd743fed81ed5de922f0b3e7769bf3b77a953af3c0779ecefd7f23` | `5ELwW5Q5vLgPKqBpRxuQwGcaGwUhYUVzEd9MhfVUzWWdhLTr` | 1,000,000 MINI |

MINI uses 12 decimal places. The Faucet dispenses 100 MINI per successful
request and enforces a 100-block cooldown.

Private authority, relayer, Faucet, Sudo, and Worker credentials are supplied
at runtime and must never be committed to this repository.
