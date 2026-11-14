# Built by packaging/linux/make-rpm.sh from a staged tree, not from source: the binary is the
# release binary (already stripped), so there is no %build and no debuginfo package. rpmbuild
# derives the Requires from the ELF headers (shared libraries and the glibc symbol versions).
%global debug_package %{nil}
%global __strip /bin/true
%global _build_id_links none

Name:           mdview
Version:        %{app_version}
Release:        1
Summary:        A light Markdown viewer
License:        MIT
URL:            https://github.com/Kaan88/markdown-viewer-rs
ExclusiveArch:  x86_64
Recommends:     libcurl
Recommends:     libwayland-client
Recommends:     xdg-desktop-portal

%description
mdview is a light, fast native Markdown viewer: tables, math, diagrams, images,
tabs, find and an outline. This package installs the viewer, mdview-engine (the
helper process it starts for diagrams and math), its desktop entry and its icon. It does not update itself; use the package manager, or the
AppImage if you want the built-in updater.

%install
mkdir -p %{buildroot}/usr
cp -a %{stage_dir}/. %{buildroot}/usr/
install -Dm644 %{license_file} %{buildroot}/usr/share/licenses/mdview/LICENSE
install -Dm644 %{readme_file} %{buildroot}/usr/share/doc/mdview/README.md

%post
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || :
command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -f /usr/share/icons/hicolor/index.theme ] && gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || :

%postun
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || :
command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -f /usr/share/icons/hicolor/index.theme ] && gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || :

%files
%license /usr/share/licenses/mdview/LICENSE
%doc /usr/share/doc/mdview/README.md
/usr/bin/mdview
/usr/bin/mdview-engine
/usr/share/applications/mdview.desktop
/usr/share/icons/hicolor/256x256/apps/mdview.png

%changelog
* %{changelog_date} Kaan88 <Kaan88@users.noreply.github.com> - %{app_version}-1
- Release %{app_version}, see https://github.com/Kaan88/markdown-viewer-rs/releases/tag/v%{app_version}
