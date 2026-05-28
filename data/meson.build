# Shipped artifacts: launcher, AppStream metadata, icon, license, channel logos.
#
# The .desktop and .metainfo.xml files are generated from .in templates by
# i18n.merge_file, which folds in any translations found in ../po based on the
# matching msgid in the .pot/.po catalogues. Plain English keys (Name=, Comment=,
# <name>, <summary>, …) are picked up automatically — no `_` underscore prefix
# needed.

desktop_file = i18n.merge_file(
  input:       application_id + '.desktop.in',
  output:      application_id + '.desktop',
  type:        'desktop',
  po_dir:      meson.project_source_root() / 'po',
  install:     true,
  install_dir: datadir / 'applications',
)

appstream_file = i18n.merge_file(
  input:       application_id + '.metainfo.xml.in',
  output:      application_id + '.metainfo.xml',
  po_dir:      meson.project_source_root() / 'po',
  install:     true,
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
    args: [desktop_file],
  )
endif

appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
  test(
    'validate-metainfo',
    appstreamcli,
    args: ['validate', '--no-net', '--explain', appstream_file],
  )
endif
