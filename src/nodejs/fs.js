import fs from 'fs';
import path from 'path';

export function get_package_json(path) {
  return fs.readFileSync(path, 'utf8');
}

export function canonicalizePath(input) {
  return path.resolve(input);
}
