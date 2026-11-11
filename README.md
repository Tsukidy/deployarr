# deployarr

deployarr is a self-hosted Windows deployment server written in Rust. A PC network-boots, the server finds the PC's order in your SQL database, and the PC installs that order's Windows edition, drivers, and applications with no technician input. Its design follows Microsoft Deployment Toolkit (MDT), which Microsoft no longer distributes.

The project is in planning. There is no code yet.

- [Architecture](docs/architecture.md) explains the components and the reason for each design decision.
- [Roadmap](docs/roadmap.md) lists the milestones and the check that ends each one.
- [MDT 8456 reference](docs/research/mdt-8456.md) records how MDT implements each feature, with file and line citations.
