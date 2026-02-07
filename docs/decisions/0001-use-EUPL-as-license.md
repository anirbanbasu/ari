---
parent: Decisions
nav_order: 1
# These are optional metadata elements. Feel free to remove any of them.
status: "accepted"
date: 2026-01-30
decision-makers:
    - abasu
consulted:
    - nacarino
informed:
    - nacarino
---
# Choose EUPL as License for ARI

## Context and Problem Statement

Everything needs to be licensed, otherwise the default copyright laws apply.
For instance, in Germany that means users may not alter anything without explicitly asking for permission.
For more information see <https://help.github.com/articles/licensing-a-repository/>.

We want to have ARI used without any hassle and that users can just go ahead and implement functionality atop ARI.

## Considered Options

* [BSD 3-Clause](https://choosealicense.com/licenses/bsd-3-clause)
* [GPL v3.0](https://choosealicense.com/licenses/gpl-3.0)
* [AGPL v3.0](https://choosealicense.com/licenses/agpl-3.0)
* [EUPL v1.2](https://choosealicense.com/licenses/eupl-1.2)

## Decision Outcome

Chosen option: "EUPL v1.2", because this license is compatible with European Union law and promotes interoperability and sharing. It is also the most copyleft-friendly license that is compatible with a wide range of other licenses, including GPL.

## Pros and Cons of the Options

### BSD 3-Clause

* Good, because it is permissive and widely used.
* Bad, because it allows proprietary derivatives, which may hinder collaboration in the open-source community.

### GPL v3.0

* Good, because it ensures that derivatives remain open source.
* Bad, because despite being copyleft, it is not the strongest copyleft license.

### AGPL v3

* Good, because it extends copyleft to network use, ensuring that even web-based applications remain open source.
* Bad, because despite it being copyleft, it is not the strongest copyleft license.

### EUPL v1.2

* Good, because it is a strong copyleft license that is compatible with a wide range of other licenses, including GPL.
* Good, because it is specifically designed to be compatible with European Union law.
* Good, because it promotes interoperability and sharing within the open-source community.
* Good, because it has official translations in multiple languages (23, at the time of writing), enhancing its accessibility. See [the EUPL 1.2 text](https://interoperable-europe.ec.europa.eu/collection/eupl/eupl-text-eupl-12).
* Bad, because it is less known outside of Europe, which may limit its adoption in other regions.

## Conclusion

We choose EUPL v1.2 as the license for ARI to promote open collaboration while ensuring compatibility with European Union law and a wide range of other licenses.
