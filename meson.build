project(
  'pintrust',
  version: '1.0.1',
  license: 'MIT',
  meson_version: '>= 0.63.0',
)

gnome = import('gnome')

cargo_build = find_program('build-aux/cargo-build.sh')

custom_target(
  'pintrust-bin',
  output: 'pintrust',
  command: [
    cargo_build,
    meson.project_source_root(),
    meson.project_build_root(),
    '@OUTPUT@',
    get_option('profile'),
  ],
  build_by_default: true,
  install: true,
  install_dir: get_option('bindir'),
)

install_data(
  'data/com.lurim412.PintRust.desktop',
  install_dir: get_option('datadir') / 'applications',
)

install_data(
  'data/com.lurim412.PintRust.metainfo.xml',
  install_dir: get_option('datadir') / 'metainfo',
)

install_data(
  'data/icons/hicolor/scalable/apps/com.lurim412.PintRust.svg',
  install_dir: get_option('datadir') / 'icons/hicolor/scalable/apps',
)

install_data(
  'data/icons/hicolor/256x256/apps/com.lurim412.PintRust.png',
  install_dir: get_option('datadir') / 'icons/hicolor/256x256/apps',
)

gnome.post_install(
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
