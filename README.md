# deployarr

deployarr is a self-hosted Windows deployment server written in Rust. A PC network-boots, the server finds what to install from a pre-staged record, an order in your SQL database, or a technician's pick from a list, and the PC installs that Windows edition, drivers, and applications. The only other input is the Windows product key from the PC's sticker, which a technician types in WinPE. The SQL database is optional. Its design follows Microsoft Deployment Toolkit (MDT), which Microsoft no longer distributes.

The project is in planning. The repository holds the design docs and an empty Cargo workspace. There is no feature code yet.

- [Architecture](docs/architecture.md) explains the components and the reason for each design decision.
- [Roadmap](docs/roadmap.md) lists the milestones and the check that ends each one.
- [MDT 8456 reference](docs/research/mdt-8456.md) records how MDT's scripts implement each feature, with file and line citations.
- [MDT 8450 binaries reference](docs/research/mdt-8450-binaries.md) records what MDT's compiled components do, including the task sequence engine.
