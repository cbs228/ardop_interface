# Guidelines for Contributors

Greetings, fellow human! We are glad that you appreciate the things that we, as
humans, enjoy doing. This planet holds many wonders to be experienced,
including:

* Snacks which are harmful to most non-humans, such as chocolate
* An atmosphere which occasionally reflects radio waves
* Laundry machines which consistently consume one of the four socks that our
  human feet require

But you're probably here about those radio waves.

This project is an experiment, unleashed all at once onto an unsuspecting
planet… with minimal concern for the consequences. Like all experiments, it is
destined for obscurity and obsolescence. If you value this experiment, help keep
it alive for awhile longer.

The human maintainer—yes, at the moment, only one—has many such experiments to
conduct. The maintainer also hopes to discover the whereabouts of that missing
sock. You can help the maintainer out by reducing the burden of maintainership
as much as possible. Specifically, we request that your contributions have a
high SIGNAL-TO-NOISE ratio.

## Contributions Sought

Activities with a **HIGH** signal-to-noise ratio include:

* **Pull requests**, especially if they either:

  1. Fix a bug; OR
  2. Implement functionality which is sought-after in open issues.

* **Detailed bug reports**. Please provide a *minimal* example code snippet or a
  written procedure which reproduces the bug. Our issue tracker only accepts
  issues pertaining to the Rust `ardop_interface` itself. Unrelated issues,
  such as the stubborn preponderance of matter over anti-matter, are best
  triaged elsewhere.

Submissions which are none of the above may be more noise than signal. The issue
tracker is for bug reports and developer coordination only. The following
activities are better suited to the ARDOP
[forums](https://ardop.groups.io/g/users/topics):

* Bug reports against the ARDOP TNC itself. The ARDOP TNC is maintained by
  different humans. Adding more humans to the loop will simply get in the way,
  so it is best to contact the TNC maintainers directly.

* Support requests, particularly for rigs or RF setups

* QSO requests and the like

The [forums](https://ardop.groups.io/g/users/topics) are full of friendly radio
enthusiasts who are also humans. You should definitely check them out.

## Pull Requests

The maintainer prefers, but does not require, that
[pull requests](https://help.github.com/en/articles/about-pull-requests) be
submitted via Github. *Smaller* pull requests are more likely to be accepted
quickly than larger pull requests. If you have Big Plans for this project, it
may be prudent to discuss them on the issue tracker… before you implement
them.

Prior to merge, your pull request should:

* Have accompanying unit tests which run with `cargo test`
* Pass the `cargo check` linter with no outstanding issues
* Include any necessary documentation changes
* Be formatted with `cargo fmt`

Test cases should not require the presence of an actual ARDOP TNC in order to
pass. Instead, known TNC behavior should be mocked. Writing good test cases can
be time-consuming, but it helps improve the overall quality of our code. And
quality code makes for happy human coders!

## Code of Conduct

Contributors are asked to abstain from any behavior which might motivate us to
adopt a formal code of conduct.

## Licensing

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.

## Conclusion

We look forward to receiving the contents of your human mind shortly. If your
contribution enhances the experiment described above, we may accept the changes…
and distribute them to the planet on which we live.

In these situations, it appears to be customary to close with,

*73, DIT DIT.*
