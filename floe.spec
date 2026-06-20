%global app_name floe
%global app_version 0.1.0
%global app_icon floe

Name:       %{app_name}
Version:    %{app_version}
Release:    1%{?dist}
Summary:    Desktop dictation app — Groq STT, push-to-talk

License:    MIT
URL:        https://github.com/user/floe
Source0:    %{app_name}-%{version}.tar.gz

# Build dependencies
BuildRequires:  nodejs >= 22
BuildRequires:  npm
BuildRequires:  rust >= 1.80
BuildRequires:  cargo
BuildRequires:  pkgconf-pkg-config
BuildRequires:  webkit2gtk4.1-devel >= 2.44
BuildRequires:  libsoup3-devel
BuildRequires:  dbus-devel
BuildRequires:  libappindicator-gtk3-devel
BuildRequires:  librsvg2-devel
BuildRequires:  patchelf
BuildRequires:  openssl-devel
BuildRequires:  gcc
BuildRequires:  glib2-devel
BuildRequires:  cairo-gobject-devel
BuildRequires:  gdk-pixbuf2-devel
BuildRequires:  at-spi2-core-devel

Requires:       webkit2gtk4.1
Requires:       dbus
Requires:       openssl
Requires:       gnome-keyring
Requires:       libappindicator-gtk3
Requires:       librsvg2

%description
Floe is a desktop dictation app that uses Groq Whisper Turbo for
speech-to-text, supporting push-to-talk via global hotkey.

%prep
%autosetup -n %{app_name}-%{version}

%build
# 1. Install frontend dependencies
npm ci

# 2. Build frontend
npm run build

# 3. Build Tauri app (Rust backend + bundling)
#    This produces the RPM under src-tauri/target/release/bundle/rpm/
cd src-tauri
cargo build --release
cd ..

%install
# Tauri's cargo build produces the bundle already.
# We just install the built binary + desktop/metadata files.
install -Dm0755 src-tauri/target/release/%{app_name} \
  %{buildroot}%{_bindir}/%{app_name}

install -Dm0644 src-tauri/icons/128x128.png \
  %{buildroot}%{_datadir}/icons/hicolor/128x128/apps/%{app_icon}.png

install -Dm0644 src-tauri/icons/32x32.png \
  %{buildroot}%{_datadir}/icons/hicolor/32x32/apps/%{app_icon}.png

# Desktop entry
cat > %{buildroot}%{_datadir}/applications/%{app_name}.desktop << DESKTOP
[Desktop Entry]
Name=Floe
Comment=Desktop dictation with Groq STT
Exec=%{_bindir}/%{app_name}
Icon=%{app_icon}
Terminal=false
Type=Application
Categories=Utility;Audio;
StartupNotify=true
DESKTOP

%files
%{_bindir}/%{app_name}
%{_datadir}/icons/hicolor/*/apps/%{app_icon}.png
%{_datadir}/applications/%{app_name}.desktop

%changelog
* Mon Jun 16 2026 Floe Team <dev@floe.app> - 0.1.0-1
- Initial RPM release
