{ pkgs, ... }:

{
  languages.rust = {
    enable = true;
    channel = "nixpkgs";
  };

  packages = [
    pkgs.git
  ];

  env.RUST_BACKTRACE = "1";

  scripts.test.exec = ''
    cargo test
  '';

  scripts.check.exec = ''
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
  '';

  enterShell = ''
    echo "devenv ready: $(rustc --version) / $(cargo --version)"
    echo "Run tests with: test"
    echo "Run full checks with: check"
  '';

  enterTest = ''
    cargo test
  '';
}
