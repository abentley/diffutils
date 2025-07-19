# flake.nix
{
  description = "A development shell for diffutils"; # Update description

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-25.05"; # Ensure this is your desired Nixpkgs channel
    rust-bin.url = "github:oxalica/rust-overlay/master"; # For rust-bin.fromRustupToolchainFile
    rust-bin.inputs.nixpkgs.follows = "nixpkgs"; # This tells rust-bin to use *your* nixpkgs
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-bin,
    }:
    let
      # Define the system architecture
      system = "x86_64-linux";
      # Import nixpkgs for the specific system
      pkgs = import nixpkgs {
        inherit system;
        # Configure rust-bin overlay to integrate with this pkgs set
        overlays = [ rust-bin.overlays.default ];
      };

      # Get the Rust toolchain from your rust-toolchain.toml
      # This will automatically pick up "nightly" from your rust-toolchain.toml
      # and provide the appropriate rustc and cargo.
      rustToolchain = pkgs.rust-bin.fromRustupToolchainFile (toString ./rust-toolchain.toml);

    in
    {
      devShells.${system}.default = pkgs.mkShell {
        # Add the rustToolchain to your buildInputs
        buildInputs = [
          rustToolchain
          pkgs.ed
          # Add any other tools you need, e.g., for C/C++ dependencies if any
          # pkgs.pkg-config
          # pkgs.openssl
        ];

        # Your shellHook will now have cargo from the selected toolchain
        shellHook = ''
          echo ">>> Entering Rust development shell (nightly toolchain) <<<"
          echo "Rust toolchain: $(rustc --version)"
          echo "Cargo version: $(cargo --version)"
          # You might want to set up PROTOC for protobufs, etc.
          # export PROTOC=$(which protoc)
        '';
      };
    };
}
