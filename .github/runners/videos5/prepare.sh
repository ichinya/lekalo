#!/usr/bin/env bash
set -euo pipefail
/usr/local/bin/trust.sh
# setup-php can disable extensions globally; restore this executor's baseline.
sudo rsync -a --delete /opt/ci-php-baseline/cli/ /etc/php/8.3/cli/
sudo update-alternatives --set php /usr/bin/php8.3 >/dev/null
sudo update-alternatives --set php-config /usr/bin/php-config8.3 >/dev/null
