{
  description = "IMAP IDLE listener that forwards new email events to a signed webhook";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          imap-idle-webhook = pkgs.rustPlatform.buildRustPackage {
            pname = "imap-idle-webhook";
            version = "0.1.0";

            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
            meta = with pkgs.lib; {
              description = "IMAP IDLE listener that forwards new email events to a signed webhook";
              homepage = "https://github.com/CYL96/imap-idle-webhook";
              license = licenses.mit;
              mainProgram = "imap-idle-webhook";
              platforms = platforms.linux;
            };
          };

          default = self.packages.${system}.imap-idle-webhook;
        });
      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/imap-idle-webhook";
        };
      });
    };
}
