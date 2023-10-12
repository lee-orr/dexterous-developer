# Introduction

This library provides an experimental hot reload system for Bevy.

## Features

- Define the reloadable areas of your game explicitly - which can include systems, components, state, events and resources (w/ some limitations)
- Reset resources to a default or pre-determined value upon reload
- serialize/deserialize your reloadable resources & components, allowing you to evolve their schemas so long as they are compatible with the de-serializer (using rmp_serde)
- mark entities to get removed on hot reload
- run systems after hot-reload
- create functions to set-up & tear down upon either entering/exiting a state or on hot reload
- only includes any hot reload capacity in your build when you explicitly enable it - such as by using the CLI launcher

## Additional Resources

We also have [API Docs](https://lee-orr.github.io/dexterous_developer/doc/dexterous_developer/index.html)

## Credits

This project was inspired by [DGriffin91's Ridiculous bevy hot reloading](https://github.com/DGriffin91/ridiculous_bevy_hot_reloading).
