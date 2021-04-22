# Blocks iterator

Iterates over Bitcoin blocks, decoding data inside Bitcoin Core's blocks directory.

Features:
* Blocks are returned in order, avoiding to follow reorgs
* Blocks comes with metadata like all block's previous outputs, allowing to compute transactions fee
* Scan takes about 1 hour for mainnet (at block 680000) and 5 minutes on testnet (at block 1972337)

Handling fee computation in memory requires good amount of ram, about 10GB for testnet and 20GB for mainnet.