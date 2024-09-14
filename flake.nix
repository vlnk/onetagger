{
  inputs = {
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = inputs: with inputs;
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) {
          inherit system;
        };

        onetagger = pkgs.callPackage ./. { inherit nixpkgs system rust-overlay; };

      in rec {
        # For `nix build` & `nix run`:
        packages = {
          onetagger = onetagger;
        };

        # For `nix develop` (optional, can be skipped):
        devShell = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [ rustc cargo lld autogen alsa-lib pkg-config  openssl libgcc glib];
          buildInputs = with pkgs; [nodejs pnpm curl gnumake pango cairo gdk-pixbuf gtk3 libsoup webkitgtk_4_1];
        };
      }
    );
}
