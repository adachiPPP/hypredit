# Maintainer: hypredit contributors
pkgname=hypredit
pkgver=0.1.0
pkgrel=1
pkgdesc="Hyprland settings and tamper control center with embedded wiki"
arch=('x86_64')
url="https://github.com/hypredit/hypredit"
license=('BSD-2-Clause')
depends=('gtk4' 'libadwaita' 'webkitgtk-6.0' 'hyprland')
makedepends=('cargo' 'git')
provides=("${pkgname}")
conflicts=("${pkgname}")
source=("${pkgname}-${pkgver}.tar.gz::${url}/archive/refs/tags/v${pkgver}.tar.gz")
sha256sums=('SKIP')

prepare() {
  cd "${pkgname}-${pkgver}"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "${pkgname}-${pkgver}"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release
}

check() {
  cd "${pkgname}-${pkgver}"
  export RUSTUP_TOOLCHAIN=stable
  cargo test --frozen
}

package() {
  cd "${pkgname}-${pkgver}"
  install -Dm755 "target/release/${pkgname}" "${pkgdir}/usr/bin/${pkgname}"
  install -Dm644 "packaging/${pkgname}.desktop" \
    "${pkgdir}/usr/share/applications/${pkgname}.desktop"
  install -Dm644 "logo.png" \
    "${pkgdir}/usr/share/icons/hicolor/512x512/apps/${pkgname}.png"
}
