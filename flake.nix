{
  description = "Koma - horizontal strip workspace for Web and Terminal";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "aarch64-darwin";
      pkgs = nixpkgs.legacyPackages.${system};
      version = "0.1.0";
      hash = "sha256-K5x19RBVrYcvQ/4rAbCcKISGDIUI3zi8M6qIZOzqiG8=";
      koma = pkgs.stdenvNoCC.mkDerivation {
        pname = "koma";
        inherit version;

        src = pkgs.fetchurl {
          url = "https://github.com/nekonata-team/koma/releases/download/v${version}/Koma-${version}-arm64.zip";
          inherit hash;
        };

        nativeBuildInputs = [ pkgs.unzip ];

        sourceRoot = ".";
        unpackPhase = ''
          unzip -q "$src" -x '__MACOSX/*' -d .
        '';

        dontConfigure = true;
        dontBuild = true;
        dontStrip = true;
        dontFixup = true;

        installPhase = ''
          runHook preInstall
          mkdir -p "$out/Applications" "$out/bin"
          cp -R Koma.app "$out/Applications/"
          ln -s "$out/Applications/Koma.app/Contents/MacOS/koma" "$out/bin/koma"
          runHook postInstall
        '';

        meta = {
          description = "Horizontal strip workspace for Web and Terminal";
          homepage = "https://github.com/nekonata-team/koma";
          platforms = [ "aarch64-darwin" ];
          mainProgram = "koma";
        };
      };
    in
    {
      packages.${system} = {
        default = koma;
        inherit koma;
      };
    };
}
