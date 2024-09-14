{ stdenv, pkgs, nixpkgs, system, makeRustPlatform, rust-overlay, nodejs, pnpm_9 }:


let client = stdenv.mkDerivation rec {
      pname = "onetagger-client";
      version = "1.7.0";

      nativeBuildInputs = [
        nodejs
      ];

      src = ./client;


      pnpmDeps = pnpm_9.fetchDeps {
        inherit pname src;
        hash = "sha256-4Hk0ZAhRvKoH7X5alSVFrCcjY1QoWAG+YDthWdSzJOM=";
      };
    };
in pkgs.rustPlatform.buildRustPackage {
  pname = "onetagger";
  version = "1.7.0";
  src = ./.;

  cargoHash = "";

  nativeBuildInputs = with pkgs; [ rustc cargo lld autogen pkg-config  openssl libgcc glib];
  buildInputs = with pkgs; [nodejs pnpm curl gnumake pango cairo gdk-pixbuf gtk3 libsoup webkitgtk_4_1 alsa-lib client];

  cargoLock = {
    lockFile = ./Cargo.lock;

    outputHashes = {
      "songrec-0.2.1" = "sha256-pQKU99x52cYQjBVctsI4gdju9neB6R1bluL76O1MZMw=";
    };
  };
}
