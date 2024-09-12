{ pkgs, nixpkgs, system, makeRustPlatform, rust-overlay, nodejs, pnpm_9 }:

{
  client = pnpm_9.fetchDeps {
    hash = "sha256-4Hk0ZAhRvKoH7X5alSVFrCcjY1QoWAG+YDthWdSzJOM=";
    sourceRoot = ./client;
  };

  server = pkgs.rustPlatform.buildRustPackage {
    pname = "app";
    version = "0.0.1";
    src = ./.;
    cargoBuildFlags = "-p app";

    cargoLock = {
      lockFile = ./Cargo.lock;
    };

    nativeBuildInputs = [ pkgs.pkg-config ];
    PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
  };
}
