# Chance: on-chain program

The Solana program behind Chance, a SOL jackpot wheel. It holds every round's deposits, takes the fee, draws the
winner from MagicBlock VRF randomness and pays out. This repository is published so anyone can check that the
program running on mainnet was built from exactly this code.

- Program: `jhEGnMU7RECUH15Z3Yo6psmURzSj8CXBDJcpZkjNYrq`
- Randomness: MagicBlock VRF (`Vrf1RNUjXmQGjmQrQLvJHs9SNkvDJEsRVFPkfSQUwGz`)

## Verify it yourself

Needs Docker and [solana-verify](https://github.com/Ellipsis-Labs/solana-verifiable-build). The build runs in the
Agave 3.1.10 image, pinned by digest so everyone builds with exactly the same tools.

```sh
solana-verify build --library-name jackpot --arch v2 \
  --base-image solanafoundation/solana-verifiable-build@sha256:f71be5ca7620b7e40933b7f1294fa44e01d08c1fc5ba1f375a2478f5a01580d3
solana-verify get-executable-hash target/deploy/jackpot.so
solana-verify get-program-hash -u https://api.mainnet-beta.solana.com jhEGnMU7RECUH15Z3Yo6psmURzSj8CXBDJcpZkjNYrq
```

The two hashes match when the deployed program is this code. Solscan shows the same check as a "Verified" badge.
