const { execSync } = require('child_process');
const fs = require('fs');

const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
const version = `v${pkg.version}`;

console.log('=======================================================');
console.log(` Preparing Release: ${version}`);
console.log('=======================================================');

try {
  // Check if tag already exists
  const existing = execSync('git tag -l ' + version).toString().trim();
  if (existing) {
    console.log(`\n⚠️  Tag ${version} already exists locally.`);
  } else {
    execSync(`git tag -a ${version} -m "Codex Switcher ${version}"`, { stdio: 'inherit' });
    console.log(`\n✔ Created git tag: ${version}`);
  }

  console.log('\n=======================================================');
  console.log(' Next Steps to Publish Release:');
  console.log(' 1. Push commits:');
  console.log('    git push');
  console.log(' 2. Push tag (triggers GitHub Actions CI release automatically):');
  console.log(`    git push origin ${version}`);
  console.log('=======================================================');
} catch (err) {
  console.error('\n✖ Release error:', err.message);
  process.exit(1);
}
