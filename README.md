# anystore

`anystore` is a polymorphic, type-safe, composable async framework for specifying API for arbitrary stores
(including databases and configuration systems). It supports addressing arbitrary type-safe hierarchies of objects.

It is best used for prototyping and configuration. It is especially good for situations when a storage system is needed,
but it's not very important, and you want to be able to change the storage provider quickly, in case the requirements change.

It is good for when you don't want to learn _yet another API_. (It also might be good if you don't want to invent _yet another API_, as it gives enough structure, primitives and utils to help you build a decent client.)

It is not indended to be used when you need high performance or reliability.

# Testing

To test the cloud integrations:

    % cargo test --all-features -- --include-ignored

You'll have to populate your .env file.
