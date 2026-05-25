# Shipped artifacts: launcher, AppStream metadata, icon, license, channel logos.

install_data(
  application_id + '.desktop',
  install_dir: datadir / 'applications',
)

install_data(
  application_id + '.metainfo.xml',
  install_dir: datadir / 'metainfo',
)

install_data(
  'icons' / 'hicolor' / 'scalable' / 'apps' / application_id + '.svg',
  install_dir: datadir / 'icons' / 'hicolor' / 'scalable' / 'apps',
)

install_data(
  '..' / 'COPYING',
  install_dir: datadir / 'licenses' / application_id,
)

# Channel logos -> $datadir/glotze/channels (== /app/share/glotze/channels in
# the sandbox). src/ui/logo.rs falls back to exactly this path, so keep it.
install_subdir(
  'channels',
  install_dir: datadir / 'glotze',
)

# --- metadata validation (soft-guarded so missing tools don't fail the build) -
desktop_file_validate = find_program('desktop-file-validate', required: false)
if desktop_file_validate.found()
  test(
    'validate-desktop',
    desktop_file_validate,
    args: [meson.current_source_dir() / application_id + '.desktop'],
  )
endif

appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
  test(
    'validate-metainfo',
    appstreamcli,
    args: [
      'validate', '--no-net', '--explain',
      meson.current_source_dir() / application_id + '.metainfo.xml',
    ],
  )
endif
