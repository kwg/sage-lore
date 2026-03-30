{
  description = "sage-lore — LLM Orchestration Engine for the SAGE Method";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = self.packages.${system}.sage-lore;

        packages.sage-lore = pkgs.rustPlatform.buildRustPackage {
          pname = "sage-lore";
          version = "1.0.0-beta";
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [ pkg-config git ];
          buildInputs = with pkgs; [ openssl ];

          # VCS tests need git config, Python tests need python3
          # Skip tests that require sandbox-unavailable tools
          checkFlags = [
            "--skip=primitives::test::verify::tests"
            "--skip=primitives::vcs::merge::tests"
            "--skip=primitives::vcs::stash::tests"
          ];

          # Git tests need a minimal git identity
          preCheck = ''
            export HOME=$(mktemp -d)
            git config --global user.email "test@example.com"
            git config --global user.name "Test"
          '';

          # Set compile-time data directory for global tier discovery (D30)
          SAGE_LORE_DATADIR = "${placeholder "out"}/share/sage-lore";

          postInstall = ''
            # Install default scrolls
            mkdir -p $out/share/sage-lore/scrolls
            cp -r scrolls/* $out/share/sage-lore/scrolls/

            # Install agents
            mkdir -p $out/share/sage-lore/agents
            cp -r agents/* $out/share/sage-lore/agents/

            # Install default config and policy
            mkdir -p $out/share/sage-lore/config/security
            cp ${./share/config/config.yaml} $out/share/sage-lore/config/config.yaml
            cp ${./share/config/security/policy.yaml} $out/share/sage-lore/config/security/policy.yaml
          '';

          meta = with pkgs.lib; {
            description = "LLM Orchestration Engine — deterministic scroll execution for AI workflows";
            homepage = "https://github.com/kai/sage-lore";
            license = licenses.mit;
            mainProgram = "sage-lore";
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            jq
            yq-go
            shellcheck
            git
            # Rust toolchain for services
            rustc
            cargo
            clippy
            rust-analyzer
            pkg-config
            openssl
            # Node for Claude Code (npm install -g)
            nodejs_22
            # Python for research scripts (FK scoring, analysis)
            python312
          ];

          NPM_PREFIX = "$HOME/.local/share/claude-code";

          shellHook = ''
            export NPM_CONFIG_PREFIX="$HOME/.local/share/claude-code"
            export PATH="$NPM_CONFIG_PREFIX/bin:$PATH"

            echo "SAGE Method dev environment loaded"
            echo "  claude: $(claude --version 2>/dev/null | head -1)"
            echo "  node: $(node --version)"
          '';
        };
      });
}
