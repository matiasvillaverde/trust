# Homebrew formula for Trust — risk-managed algorithmic trading CLI.
#
# Install:  brew install matiasvillaverde/tap/trust
# Or local: brew install --formula Formula/trust.rb
#
# NOTE: Update the `url` and `sha256` values after each release.
#       Run `shasum -a 256 <archive>` to compute the checksum.

class Trust < Formula
  desc "Risk-managed algorithmic trading CLI"
  homepage "https://github.com/matiasvillaverde/trust"
  version "0.3.4"
  license "GPL-3.0-only"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/matiasvillaverde/trust/releases/download/v#{version}/v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER" # fill after release
    else
      url "https://github.com/matiasvillaverde/trust/releases/download/v#{version}/v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER" # fill after release
    end
  end

  on_linux do
    url "https://github.com/matiasvillaverde/trust/releases/download/v#{version}/v#{version}-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "PLACEHOLDER" # fill after release

    depends_on "dbus"
  end

  def install
    bin.install "trust"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/trust --version")
  end
end
