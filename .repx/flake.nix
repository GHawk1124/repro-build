{
  description = "Reproducible Rust cross-build for repx";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachSystem [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ] (system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs { inherit system overlays; };
        lib = pkgs.lib;

        # Cross-package-sets
        pkgsCrossAarch64 = pkgs.pkgsCross.aarch64-multiplatform;
        pkgsCrossWindows = pkgs.pkgsCross.mingwW64;
        pkgsCrossWindowsStatic = import nixpkgs {
          inherit system overlays;
          crossSystem = {
            config = "x86_64-w64-mingw32";
            libc = "msvcrt";
            platform = { useAndroidPrebuilt = false; };
          };
        };
        pkgsStatic = import nixpkgs {
          inherit system overlays;
          crossSystem = { config = "x86_64-unknown-linux-musl"; };
        };
        pkgsAarch64Static = import nixpkgs {
          inherit system overlays;
          crossSystem = { config = "aarch64-unknown-linux-musl"; };
        };

        # Helper function to get packages from the extra_packages list
        getExtraPackages = targetPkgs: 
          let
            # Extra packages specified by the user
            extraPackageNames = [];
            
            # Function to safely get a package, returning null if it doesn't exist
            safeGetPackage = name: 
              if lib.hasAttr name targetPkgs then
                lib.getAttr name targetPkgs
              else
                builtins.trace "Warning: Package '${name}' not found in nixpkgs, skipping..." null;
          in
            builtins.filter (pkg: pkg != null) (map safeGetPackage extraPackageNames);

        # Generic builder for GNU / Musl / MinGW
        buildFor = { targetSystem, targetTriple, needsWine ? false
          , staticBuild ? false }:
          let
            # Map gcc -dumpmachine style to pkgs
            nixTargetSystem =
              if targetTriple == "x86_64-unknown-linux-musl" then "x86_64-linux"
              else if targetTriple == "aarch64-unknown-linux-musl" then "aarch64-linux"
              else if targetTriple == "x86_64-pc-windows-gnu" then "x86_64-windows"
              else if targetTriple == "aarch64-pc-windows-gnu" then "aarch64-windows"
              else targetSystem; # Default to what was passed for -linux-gnu

            targetPkgs = # This section determines the Nix package set based on the Rust targetTriple and staticBuild
              if staticBuild && targetTriple == "x86_64-unknown-linux-gnu" then pkgsStatic # x86_64-linux-musl
              else if staticBuild && targetTriple == "aarch64-unknown-linux-gnu" then pkgsAarch64Static # aarch64-linux-musl
              else if targetTriple == "x86_64-pc-windows-gnu" then pkgsCrossWindows # x86_64-w64-mingw32
              else if targetTriple == "aarch64-pc-windows-gnu" then pkgs.pkgsCross.aarch64-multiplatform-windows # Placeholder
              else pkgs; # for x86_64-linux-gnu, aarch64-linux-gnu

            actualTriple = # This is the Rust triple used by rust-bin and CARGO_BUILD_TARGET
              if staticBuild && targetTriple == "x86_64-unknown-linux-gnu" then "x86_64-unknown-linux-musl"
              else if staticBuild && targetTriple == "aarch64-unknown-linux-gnu" then "aarch64-unknown-linux-musl"
              # For windows-gnu, static is handled via RUSTFLAGS, so actualTriple remains x86_64-pc-windows-gnu
              else targetTriple;

            rustBin = pkgs.rust-bin.stable.latest.default.override {
              targets = [ actualTriple ];
            };

            # Get extra packages for this target
            extraPackages = getExtraPackages targetPkgs;

            # Build inputs are only the packages specified by the user
            buildInputs = extraPackages;
            
            # Native build inputs include only essential tools and user-specified packages
            nativeBuildInputs = 
              (if needsWine then [ pkgs.wine ] else [ ])
              ++ (if targetTriple == "aarch64-unknown-linux-gnu" then [ pkgs.qemu ] else [ ]);

            rustFlags =
              if staticBuild && (actualTriple == "x86_64-unknown-linux-musl" || actualTriple == "aarch64-unknown-linux-musl") then "-C target-feature=+crt-static"
              else if targetTriple == "x86_64-pc-windows-gnu" && staticBuild then "-C target-feature=+crt-static -C linker=${pkgsCrossWindowsStatic.stdenv.cc.targetPrefix}gcc -C link-args=-static"
              else if targetTriple == "x86_64-pc-windows-gnu" then "-C linker=${pkgsCrossWindows.stdenv.cc.targetPrefix}gcc"
              else "";

            extraEnv = builtins.listToAttrs (lib.concatLists [
              (if targetTriple == "x86_64-unknown-linux-gnu"
              && staticBuild then [{
                name = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER";
                value = "${targetPkgs.stdenv.cc.targetPrefix}cc";
              }] else
                [ ])
              (if targetTriple == "aarch64-unknown-linux-gnu"
              && staticBuild then [
                {
                  name = "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER";
                  value = "${targetPkgs.stdenv.cc.targetPrefix}cc";
                }
                {
                  name = "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER";
                  value = "qemu-aarch64";
                }
              ] else
                [ ])
              (if targetTriple == "x86_64-pc-windows-gnu" then [{
                name = "CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER";
                value = "${
                    (if staticBuild then
                      pkgsCrossWindowsStatic
                    else
                      pkgsCrossWindows).stdenv.cc.targetPrefix
                  }gcc";
              }] else
                [ ])
            ]);

          in targetPkgs.rustPlatform.buildRustPackage rec {
            pname = "repx";
            version = "0.1.0";
            src = pkgs.lib.cleanSourceWith {
              src = ../.;
              filter = path: type:
                let baseName = baseNameOf path; in
                  (type == "directory" && baseName != "target" && baseName != ".git" && baseName != "result" && baseName != ".repx") ||
                  (type == "directory" && baseName == "templates") ||
                  (type == "regular" && (
                    pkgs.lib.hasSuffix ".rs" baseName ||
                    pkgs.lib.hasSuffix ".toml" baseName ||
                    pkgs.lib.hasSuffix ".lock" baseName ||
                    pkgs.lib.hasSuffix ".md" baseName ||
                    pkgs.lib.hasSuffix ".tera" baseName ||
                    baseName == "LICENSE" ||
                    baseName == ".gitignore"
                  ));
            };
            cargoLock = { lockFile = ../Cargo.lock; };
            release = true;

            # Targeted build
            CARGO_BUILD_TARGET = actualTriple;

            inherit buildInputs nativeBuildInputs;

            # Optimize
            CARGO_PROFILE_RELEASE_LTO = "true";
            CARGO_PROFILE_RELEASE_OPT_LEVEL = "s";
            CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1";
            CARGO_PROFILE_RELEASE_PANIC = "abort";
            CARGO_PROFILE_RELEASE_STRIP = "true";

            # Static if requested
            RUSTFLAGS = rustFlags;

            # Windows-specific install phase to handle .exe files
            installPhase = if (targetTriple == "x86_64-pc-windows-gnu" || targetTriple == "aarch64-pc-windows-gnu") then ''
              mkdir -p $out/bin

              # More verbose debugging
              echo "Contents of target directory:"
              find target -type d | sort

              echo "Looking for .exe files:"
              find target -name "*.exe" || echo "No .exe files found"

              # Try multiple possible locations
              if [ -f "target/${actualTriple}/release/repx.exe" ]; then
                echo "Found .exe at expected location"
                cp target/${actualTriple}/release/repx.exe $out/bin/
              elif [ -f "target/release/repx.exe" ]; then
                echo "Found .exe in target/release"
                cp target/release/repx.exe $out/bin/
              else
                echo "Searching for repx.exe in all locations"
                find target -name "repx.exe" -exec cp {} $out/bin/ \; || echo "No repx.exe found anywhere"

                echo "Copying any .exe files found as fallback"
                find target -name "*.exe" -exec cp {} $out/bin/ \; || echo "No .exe files found at all"
              fi

              # Check what was actually installed
              echo "Contents of $out/bin:"
              ls -la $out/bin/
            '' else null;

            # Extra vars
            passthru = extraEnv;
          };

      in {
        # Conditionally define packages based on the system to reduce evaluation overhead
        packages =
          # Linux systems can build for all targets (native + cross-compilation)
          if (system == "x86_64-linux" || system == "aarch64-linux") then {
            # Native Linux builds
            "x86_64-linux-gnu" = buildFor {
              targetSystem = "x86_64-linux";
              targetTriple = "x86_64-unknown-linux-gnu";
            };
            "aarch64-linux-gnu" = buildFor {
              targetSystem = "aarch64-linux";
              targetTriple = "aarch64-unknown-linux-gnu";
            };

            # Static musl builds
            "x86_64-linux-musl" = buildFor {
              targetSystem = "x86_64-linux"; # Original system for pkgs derivation
              targetTriple = "x86_64-unknown-linux-gnu"; # Rust GNU triple that gets converted to musl
              staticBuild = true;
            };
            "aarch64-linux-musl" = buildFor {
              targetSystem = "aarch64-linux";
              targetTriple = "aarch64-unknown-linux-gnu";
              staticBuild = true;
            };

            # Windows builds (GNU default)
            "x86_64-w64-mingw32" = buildFor {
              targetSystem = "x86_64-windows"; # Nix system string
              targetTriple = "x86_64-pc-windows-gnu"; # Rust triple
              needsWine = true;
            };

            # Add a static MinGW build option
            "x86_64-w64-mingw32-static" = buildFor {
              targetSystem = "x86_64-windows";
              targetTriple = "x86_64-pc-windows-gnu";
              needsWine = true;
              staticBuild = true;
            };
            "aarch64-w64-mingw32" = buildFor {
              targetSystem = "aarch64-windows";
              targetTriple = "aarch64-pc-windows-gnu"; # Assuming this is the Rust triple for ARM Windows GNU
              needsWine = true; # May need QEMU as well or instead depending on host
            };

            # Windows MSVC builds (uses pkgs.stdenv.mkDerivation directly, not buildFor)
            "x86_64-pc-windows-msvc" = pkgs.stdenv.mkDerivation {
              pname = "repx-msvc";
              version = "0.1.0";
              src = pkgs.lib.cleanSourceWith {
                src = ../.;
                filter = path: type:
                  let baseName = baseNameOf path; in
                    (type == "directory" && baseName != "target" && baseName != ".git" && baseName != "result" && baseName != ".repx") ||
                    (type == "directory" && baseName == "templates") ||
                    (type == "regular" && (
                      pkgs.lib.hasSuffix ".rs" baseName ||
                      pkgs.lib.hasSuffix ".toml" baseName ||
                      pkgs.lib.hasSuffix ".lock" baseName ||
                      pkgs.lib.hasSuffix ".md" baseName ||
                      pkgs.lib.hasSuffix ".tera" baseName ||
                      baseName == "LICENSE" ||
                      baseName == ".gitignore"
                    ));
              };

              nativeBuildInputs = [
                (pkgs.rust-bin.stable.latest.default.override {
                  targets = [ "x86_64-pc-windows-msvc" ];
                })
                pkgs.cargo-xwin
                pkgs.clang
                pkgs.llvmPackages.lld
                pkgs.wine
              ] ++ (getExtraPackages pkgs);

              buildPhase = ''
                export CARGO_HOME=$PWD/.cargo
                mkdir -p $CARGO_HOME/registry $CARGO_HOME/git
                export SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
                export NIX_SSL_CERT_FILE=$SSL_CERT_FILE
                export XWIN_ACCEPT_LICENSE=yes
                export XWIN_CACHE_DIR=$PWD/.cache/xwin
                cargo xwin build \
                    --release \
                    --locked \
                    --target x86_64-pc-windows-msvc
              '';

              installPhase = ''
                mkdir -p $out/bin
                find target -type f -executable -name "*.exe" || echo "No executables found"
                cp target/x86_64-pc-windows-msvc/release/repx{,.exe} $out/bin/ || true

                # If it's a library, install that instead
                mkdir -p $out/lib
                find target -name "*.dll" -o -name "*.lib" -o -name "*.a" || echo "No libraries found"
                cp target/x86_64-pc-windows-msvc/release/*.{dll,lib} $out/lib/ 2>/dev/null || true
              '';
            };
          }
          # macOS systems only build native targets (cross-compilation is complex on macOS)
          else if (system == "x86_64-darwin") then {
            "x86_64-apple-darwin" = buildFor {
              targetSystem = "x86_64-darwin";
              targetTriple = "x86_64-apple-darwin";
            };
          }
          else if (system == "aarch64-darwin") then {
            "aarch64-apple-darwin" = buildFor {
              targetSystem = "aarch64-darwin";
              targetTriple = "aarch64-apple-darwin";
            };
          }
          # Fallback for other systems
          else { };

        # Default package points to the native build for the current system
        default =
          if (system == "x86_64-linux") then self.packages.${system}."x86_64-linux-gnu"
          else if (system == "aarch64-linux") then self.packages.${system}."aarch64-linux-gnu"
          else if (system == "x86_64-darwin") then self.packages.${system}."x86_64-apple-darwin"
          else if (system == "aarch64-darwin") then self.packages.${system}."aarch64-apple-darwin"
          else throw "Unsupported system: ${system}";

        # ——— Dev-Shells ———
        # Conditionally define dev shells based on the system
        devShells = {
          # Default shell available on all systems
          default = pkgs.mkShell {
            nativeBuildInputs = [ pkgs.rust-bin.stable.latest.default ];
            buildInputs = getExtraPackages pkgs;
          };
        } // (
          # Linux systems get cross-compilation dev shells
          if (system == "x86_64-linux" || system == "aarch64-linux") then {
            "aarch64-linux-gnu" = pkgs.mkShell {
              nativeBuildInputs = [
                (rust-overlay.lib.mkRustBin { }
                  pkgsCrossAarch64.buildPackages.stable.latest.default.override {
                    targets = [ "aarch64-unknown-linux-gnu" ];
                  })
                pkgs.qemu
              ];
              buildInputs = getExtraPackages pkgs;
              shellHook = ''
                export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=${pkgsCrossAarch64.stdenv.cc.targetPrefix}cc
                export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER=qemu-aarch64
              '';
            };

            "x86_64-linux-musl" = pkgs.mkShell {
              nativeBuildInputs = [
                (pkgsStatic.rust-bin.stable.latest.default.override {
                  targets = [ "x86_64-unknown-linux-musl" ];
                })
              ];
              buildInputs = getExtraPackages pkgsStatic;
              shellHook = ''
                export RUSTFLAGS="-C target-feature=+crt-static"
                export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=${pkgsStatic.stdenv.cc.targetPrefix}cc
              '';
            };

            "x86_64-w64-mingw32" = pkgs.mkShell {
              nativeBuildInputs = [
                (pkgs.rust-bin.stable.latest.default.override {
                  targets = [ "x86_64-pc-windows-gnu" ];
                })
                pkgs.wine
              ];
              buildInputs = getExtraPackages pkgsCrossWindowsStatic;
              shellHook = ''
                export RUSTFLAGS="-C target-feature=+crt-static -C linker=x86_64-w64-mingw32-gcc -C link-args=-static"
                export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=${pkgsCrossWindowsStatic.stdenv.cc.targetPrefix}gcc
              '';
            };

            "x86_64-pc-windows-msvc" = pkgs.mkShell {
              nativeBuildInputs = [
                (pkgs.rust-bin.stable.latest.default.override {
                  targets = [ "x86_64-pc-windows-msvc" ];
                })
                pkgs.cargo-xwin
                pkgs.clang
                pkgs.llvmPackages.lld
                pkgs.wine
              ] ++ (getExtraPackages pkgs);
              shellHook = ''
                export XWIN_ACCEPT_LICENSE=yes
                export XWIN_CACHE_DIR=$PWD/.cache/xwin
                echo "Run → cargo xwin build --target x86_64-pc-windows-msvc"
              '';
            };
          }
          # macOS systems only get basic dev shells
          else { }
        );
      });
}
