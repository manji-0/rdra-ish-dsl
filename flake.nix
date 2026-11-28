{
  description = "rdra-ish: RDRA-inspired DSL compiler and diagram generator";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
      # TLC 1.8.0 on JDK 25, the newest JDK nixpkgs packages, with a class-data-sharing archive
      # of the classes TLC loads (#309). nixpkgs still packages TLC 1.7.4, so the package
      # takes the jar of the v1.8.0 release instead, pinned by its hash.
      #
      # TLC 1.8.0 is a pre-release that upstream rebuilds from master under the same tag.
      # This hash is the build of 2026-11-19 (rev bc1b506, `TLC2 Version 2026.11.19.180144`),
      # which this repository's release `tlc-1.8.0-bc1b506` keeps with upstream's MIT license
      # text, so the download does not depend on upstream keeping it. Upstream's URL comes
      # second; once it serves another build, Nix rejects that build on the hash. To move to
      # a newer build, publish it the same way and change the URLs and the hash together.
      #
      # TLC 1.7.x cannot run on JDK 21 or later: its BufferedRandomAccessFile.read(byte[], int,
      # int) breaks when it reads back a trace record still in its write buffer, which
      # RandomAccessFile.readLong reaches from JDK 21 on (#314). TLC 1.8.0 rewrote that read.
      #
      # The archive is recorded from one run of nix/tlc-cds/Train.tla against the jar's store
      # path. The JVM checks that the jar and the JDK are the ones it was recorded with and
      # ignores it when they are not, so the archive can only go unused, not change a run.
      # `-XX:+UseParallelGC` stays as nixpkgs sets it: TLC asks for it, and a second GC flag
      # stops the JVM.
      tlcFor =
        pkgs:
        let
          jdk = pkgs.jdk25_headless;
          tlaplus = (pkgs.tlaplus.override { jre8 = jdk; }).overrideAttrs {
            version = "1.8.0";
            src = pkgs.fetchurl {
              urls = [
                "https://github.com/manji-0/rdra-ish-dsl/releases/download/tlc-1.8.0-bc1b506/tla2tools.jar"
                "https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar"
              ];
              hash = "sha256-/f/vJH9WPvbtv9RJWNKoLbeedWTgfLZzJi2JY1P6ZxM=";
            };
          };
          jar = "${tlaplus}/share/java/tla2tools.jar";
          archive = pkgs.runCommand "tla2tools-${tlaplus.version}-cds" { } ''
            mkdir -p $out work
            cp ${./nix/tlc-cds}/Train.tla ${./nix/tlc-cds}/Train.cfg work/
            cd work
            # Runs TLC on the training spec with the JVM flags given; TLC exits 13 on the
            # violated property, as intended. -Xmx1g as `verify` gives a cone: an archive
            # holds one compressed-oops mode, and a builder's default heap past 32 GiB would
            # record the one a `verify` run never maps.
            train() {
              rm -rf states
              status=0
              ${jdk}/bin/java -XX:+UseParallelGC -XX:-UsePerfData -Xmx1g "$@" -Djava.io.tmpdir=$PWD \
                -cp ${jar} tlc2.TLC -workers 1 -metadir $PWD/states \
                -config Train.cfg Train.tla > train.log 2>&1 || status=$?
              if [ $status -ne 13 ] || ! grep -q '^Error: Temporal property ShippedReopens was violated.$' train.log; then
                cat train.log
                exit 1
              fi
            }
            train -XX:ArchiveClassesAtExit=$out/tla2tools.jsa
            # The archive must map: -Xshare:on stops the JVM where the default would skip it.
            train -Xshare:on -XX:SharedArchiveFile=$out/tla2tools.jsa
          '';
          # The driver that runs many TLC jobs in one JVM (#310). It is written for Java 8, the
          # oldest JVM TLC 1.7.4 runs on, and calls nothing TLC 1.8.0 dropped.
          batch = pkgs.runCommand "rdra-tlc-batch" { } ''
            mkdir -p $out
            ${jdk}/bin/javac --release 8 -Xlint:-options -d $out ${./nix/tlc-batch}/RdraTlcBatch.java
          '';
        in
        pkgs.runCommand "tlaplus-${tlaplus.version}-jdk25"
          {
            nativeBuildInputs = [ pkgs.makeWrapper ];
            inherit (tlaplus) meta;
          }
          ''
            mkdir -p $out/bin
            ln -s ${tlaplus}/share $out/share
            makeWrapper ${jdk}/bin/java $out/bin/tlc \
              --add-flags "-XX:+UseParallelGC -XX:SharedArchiveFile=${archive}/tla2tools.jsa -cp ${jar} tlc2.TLC"
            # The same JVM and collector as `tlc`. Without the archive: the driver loads TLC
            # through a class loader per job, and the JVM shares archived classes with its
            # own loaders only. A JVM out of memory ends at once, for its caller to see, and
            # soft references are cleared at each GC, without which no job's classes unload
            # (nix/tlc-batch/RdraTlcBatch.java).
            makeWrapper ${jdk}/bin/java $out/bin/tlc-batch \
              --add-flags "-XX:+UseParallelGC -XX:+ExitOnOutOfMemoryError -XX:SoftRefLRUPolicyMSPerMB=0 -cp ${batch} RdraTlcBatch ${jar}"
            for tool in tlasany pcal tlatex; do
              ln -s ${tlaplus}/bin/$tool $out/bin/$tool
            done
          '';
    in
    {
      packages = forAllSystems (pkgs: {
        rdra-ish = pkgs.rustPlatform.buildRustPackage {
          pname = "rdra-ish";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "-p"
            "rdra-ish-cli"
            "-p"
            "rdra-ish-lsp"
          ];
          # The workspace tests call python3, tlc, z3, tsc and go; CI runs them in the devShell.
          doCheck = false;
          # `verify --with-tlc` runs tlc, and `verify` runs z3, from PATH. Appended, so a tlc or z3
          # the user already has comes first.
          nativeBuildInputs = [ pkgs.makeWrapper ];
          postInstall = ''
            wrapProgram $out/bin/rdra-ish \
              --suffix PATH : ${
                pkgs.lib.makeBinPath [
                  (tlcFor pkgs)
                  pkgs.z3
                ]
              }
          '';
          meta = {
            description = "RDRA-inspired DSL compiler and diagram generator";
            license = pkgs.lib.licenses.mit;
            mainProgram = "rdra-ish";
          };
        };

        default = self.packages.${pkgs.stdenv.hostPlatform.system}.rdra-ish;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
            })
            pkgs.cargo-insta
            # The workspace tests (scripts/diff-test.sh, .config/nextest.toml)
            pkgs.cargo-nextest

            # Python packaging (maturin bin bindings, uv publish flow)
            pkgs.maturin
            pkgs.uv
            pkgs.python3

            # Formal verification (TLC)
            pkgs.jdk25_headless
            (tlcFor pkgs)
            # `verify --prove` (tests/prove_cli.rs, tests/proof_equivalence.rs)
            pkgs.z3

            # VS Code extension (editors/vscode)
            pkgs.nodejs

            # Compile checks of `rdra-ish codegen` output (tests/codegen_builds.rs)
            pkgs.typescript
            pkgs.go

            # Lean proofs (proofs/lean). elan installs the toolchain
            # proofs/lean/lean-toolchain pins, on the first lake build.
            # nixpkgs' lean4 is a different pin.
            pkgs.elan

            pkgs.pre-commit
            # Mutation run over the liveness ladder (.cargo/mutants.toml, .github/workflows/mutants.yml)
            pkgs.cargo-mutants
          ]
          # GNU time for scripts/verify-samples.sh (macOS has BSD /usr/bin/time).
          ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.time ];

          JAVA_HOME = "${pkgs.jdk25_headless}";

          # A GOROOT from the user's shell (a version manager's Go, say) points the flake's
          # `go` at another toolchain's standard library, and the codegen build tests then
          # fail on a version mismatch. Without it, `go` finds its own root.
          shellHook = ''
            unset GOROOT
          '';
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
