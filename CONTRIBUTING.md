# Contribution Guidelines

First and foremost: Thank you for showing interest in contributing to bevy_ggrs! Make sure to read the [Code of Conduct](./CODE_OF_CONDUCT.md).

If you have a cool example or showcase of bevy_ggrs in use, let us know so your project can be highlighted!

## Create an Issue

- [Bug report](https://github.com/gschup/bevy_ggrs/issues/new?assignees=&labels=bug&template=bug_report.md&title=)
- [Feature request](https://github.com/gschup/bevy_ggrs/issues/new?assignees=&labels=enhancement&template=feature_request.md&title=)

## Contribute to bevy_ggrs

Please send a [GitHub Pull Request](https://github.com/gschup/bevy_ggrs/pull/new/main) with a clear list of what you've done
(read more about [pull requests](http://help.github.com/pull-requests/)). When you send a pull request,
it would be great if you wrote unit- or integration tests for your changes. Please format your code via `cargo fmt` and
make sure all of your commits are atomic (one feature per commit).

Always write a clear log message for your commits. One-line messages are fine for small changes, but bigger changes should look like this:

    $ git commit -m "prefix: brief summary of the commit

    A paragraph describing what changed and its impact."

With the following prefixes commonly used:

- `feat`: for new features
- `fix`: for fixing a bug
- `doc`: for adding/changing documentation
- `test`: for adding/changing tests
- `chore`: for any minor code cleanups

More about the [GitHub flow](https://guides.github.com/introduction/flow/).
More about the [Conventional Commits Specification](https://www.conventionalcommits.org/en/v1.0.0/)

## Guidelines

- **Bevy version compatibility** — bevy_ggrs tracks the latest stable Bevy release. PRs that bump the Bevy version are welcome once the new release is out.
- **No `cargo doc` warnings** — the codebase has `#![warn(missing_docs)]` enabled. New public items must have doc comments.
- **Run `cargo fmt`** before committing.
- **Run `cargo test`** to make sure existing tests pass.
- For architecture questions, see [docs/architecture.md](./docs/architecture.md).
