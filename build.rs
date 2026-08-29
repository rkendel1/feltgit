// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation: version 2 of the License, dated June 1991.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, see <https://www.gnu.org/licenses/>.

fn main() {
    // Check if git-integration feature is enabled by looking at CARGO_FEATURE_GIT_INTEGRATION
    let has_git_integration = std::env::var("CARGO_FEATURE_GIT_INTEGRATION").is_ok();
    
    // Only link to libgit when the git-integration feature is enabled and libgit.a exists
    if has_git_integration && std::path::Path::new("libgit.a").exists() {
        println!("cargo:rustc-link-search=.");
        println!("cargo:rustc-link-lib=git");
        println!("cargo:rustc-link-lib=z");
    }
}




