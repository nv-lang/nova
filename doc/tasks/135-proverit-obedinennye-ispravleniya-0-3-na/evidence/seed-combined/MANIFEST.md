# Combined local seed source MANIFEST (task 135)

NOT FOR PUBLICATION: a local, undeliverable seed candidate combining the
accepted fixes 130 (anonymous constructors), 131 (extern "C"), 134 (classes
A/B) over the pinned B1 base `3729abcce42ec2d94efc898ec6eafe91ff6210d5`.
No tag, no release asset, no change to SEED_* or the official descriptor.

Layer counts: base 173 files, head 26, backport 7, merged 12
(total 218 files under novac/src). std/ and the rest of the tree are base
blobs; driver-facing assets: carina-gate.py and nova_rt are base content
(submodule pins identical base..HEAD), the carina descriptor is the OFFICIAL
one at HEAD (the base commit's descriptor names the older 0.2.0 seed and
fails verify-seed against the driver's 0.2.1 pins).

Merge recipe for the twelve `merged` files: `git merge-file` with
current=base, ancestor=pre-fix (71d8a35bd^ for 131's files, 5aabbb4b4^ for
134's), other=HEAD; task-92 hunks are removed by the merge itself
(ancestor->current deletes them); the 26 conflicts were resolved by hand
keeping base+fix and dropping task-92 lines (judge_fn_decl/walk_syntax of
decl_forms.nv dropped whole; task 131's row-passing moved into check.nv's
FnDecl arm, where base judged extern declarations). ice_at pins in the twelve
were renumbered to the physical line of each string literal.

| file | sha256 | layer |
|---|---|---|
| `atomics/atomics.nv` | `c0db755253a9bf3c2993b109f6a8817c73c8ce4ee5fc9a5e3358072b3510570f` | base |
| `builtins/builtins.nv` | `83efe096db16b851d3c447fc9d436a0986d535a042f8c7c7bf0350691fe8850c` | merged |
| `check/arg_expect.nv` | `dd0a98dc57f05c9fe43cdb320d5817d554200ad48913318456be9bd8759a3c62` | base |
| `check/arm_fold.nv` | `74bad641a0273d643970befd217ab40f02125c3f3061044b26a4791a104fe09f` | base |
| `check/assert_call.nv` | `981103e291c15f410703af9b80cf69c0360ad8fc6989135eb0644609a272dc58` | base |
| `check/assign_rules.nv` | `e203d899cc145487659f2f31473e89793e1aabb43d56fb7771f11591398ad3bf` | base |
| `check/binds.nv` | `330209ff999512200d78fe87033a5857ef05a625bf781f9154f8cd2962e20905` | backport |
| `check/bounds.nv` | `a2549622ae24a009b1aae19fe316e496e15f81d55ee149dcdc11b84eb53d428c` | base |
| `check/calls.nv` | `5d5afe71241429758aec270b1f168cbc505bc66087f61cc41173eed9cfd10094` | backport |
| `check/cast_pairs.nv` | `ff9bcd59545f4d2b341e6d8204547fe457bce9304661eca03afeede4601ee6a3` | base |
| `check/casts.nv` | `66e2cea3930435a726dd2b028437f4afc9483a479dee548128c69864e6fc4f17` | head |
| `check/check.nv` | `203bc2b2bd7f932075b072e095fb4e620aab2008d0c8f8bd35fb51f7721a7c3a` | merged |
| `check/check_test.nv` | `96e66987e59b745e35672e64005d7afadd09f82caaf77c87f087f2640fa27faf` | base |
| `check/checker_type.nv` | `661adc0a481e73dafeb2358933dc4d15de89836b4a7ce187ed7337599b62fa44` | base |
| `check/cond_forms.nv` | `92cc8985b15b318f57728654af97e68bdc9d4f2dc27c10c11e1ba64079a4664f` | base |
| `check/const_types.nv` | `52d07e097865109d313897a9dc9281fbc3b7c529b0c585022f1300fbe7136ea1` | backport |
| `check/consume_flow.nv` | `c39b59a1583f31cfe2bf8c4d8775399daeaf4b287b0a3be3493a81535a9dd831` | base |
| `check/consume_paths.nv` | `0bf665338ff46d49c3567966c0f4afea1869dc3a5f3b434b8de44ddf04c0441a` | base |
| `check/consume_shapes.nv` | `a3270f37bfb243940ca1dc0d3bae4d9b0a7b883fd4aa0b22d681a1f2c31cb50e` | merged |
| `check/ctor_calls.nv` | `1d62ed46d1db6650bbc0a3291c3c8fbcef5af7026ad72310ecafc79c05b9b83f` | head |
| `check/decl_forms.nv` | `185e99d7aab7cd4b7211bfbf19483cad830e09459ef600d3d8fe61d537a1174c` | merged |
| `check/derive_serde.nv` | `e272d0d9d7a723d0ed0948f74bf8f82860c6560ecc5ccae19156bee53c5248fc` | base |
| `check/destructure.nv` | `3e5c76b8b33dc6f7648f69718d53f7d1efaa1ec2a2a2aac7c92646b9074acaf5` | base |
| `check/diagnostics.nv` | `dff369492ba8e7e9b03e05b21864d74ebdf4086602e088425e18bb47f834ed35` | base |
| `check/disj_pattern.nv` | `f73d47ff2ed4e332df1fdefa73b4cafdcc46f00944d32e056a2ad8b3864bcd75` | base |
| `check/embed_call.nv` | `654bd472e7dc62752177e2fc41754e69691881281708e38faeb34e11a5687e43` | base |
| `check/exhaustive.nv` | `e1648249a356da23f3b8ef1af6f28a27342c659455b8c9817065d90550df68f6` | base |
| `check/exprs.nv` | `cbf977c78807af7bc60dedc9501d71a0ba2e6486793875b8af8ef89f6ca6d628` | backport |
| `check/extern_rules.nv` | `5260272eaf2cd542133c2472041608c546dcd41f5d5812ca94725ec73eb67b3f` | head (new file) |
| `check/for_iter.nv` | `0276e1ef3bbdef8e1a970058bcfe9212b77e707999d16879511aa95ce9582256` | base |
| `check/handed_default.nv` | `fae7f92b3db69ad2632c89a9ca38de96d183c16ef9c51f0a5c897156a345c217` | base |
| `check/handler.nv` | `3c96c231145ba0220a9ffdf161b4baf65e5052f4a1176d865b81a73e7883710b` | base |
| `check/index_key_test.nv` | `72ddab0a35f44f2028fa15e3cdf0864eddebad4abe759fe0868c1a2876df96d9` | base |
| `check/index_typing.nv` | `dc6acb07d35b53f6387f4f86eae52242ed2c8469f042eff0f41672117e7dd576` | base |
| `check/is_expr.nv` | `dd5a69e15b0874579d0ba2a5ed22eb4a919da12801c9ca4439e69de6dd2fb0c8` | base |
| `check/literal_rules.nv` | `ef302668ba047aece0f1253bb7d9f8cfed7b139bb5b3629ca2b0f264208d4986` | base |
| `check/match_arms.nv` | `6466b8bc8f4e39cb01acd3984022baf14ca661b810d0d9b5ad61a59be9ba4278` | base |
| `check/match_position.nv` | `4384094ca399c45f6e7a9b7b1b31143e7897b761eb3c6ee9cd208bcbb35d2e2e` | base |
| `check/messages.nv` | `02c74308b79aa9b457861a2133fb2a179ac782c968a69662db1c09f1afd7cd98` | head |
| `check/methods.nv` | `19e58dca834639c8aa7848ac3fc3f104b202a37b4d5f2646731e69954cef5dd6` | base |
| `check/module_values.nv` | `a84170b5e839c978d6c1c3beed48f79a8f64523a96d9cb1df00228b0d2043ae6` | backport |
| `check/mut_args.nv` | `cf26efdfcceaf5017859b4812b72a6dcbf1f67f3c8029468e47d64c37151b520` | head |
| `check/numeric_positions.nv` | `5249eee20c09c5511fa10a84c5b5873b2db0085220e6e3ba18882ee6090d5eeb` | base |
| `check/operators.nv` | `841d0fdb65b4555cda95494ae354cc9d59f2f99cadf081bc51042ce66b15ed98` | base |
| `check/option_rules.nv` | `259c8d271e0c42e322474702c0a4e1562118064a90812857c22b5482a002e2fc` | backport |
| `check/params.nv` | `c543970e327066bdc6c5994f998034c1ba71fa013b4508ef39479f5b5eefe353` | head |
| `check/pattern_position.nv` | `5036da90fdf907631dd414a9a48d5a04547d3496f2b1a750ac82f9dbe162dadf` | base |
| `check/record_variant.nv` | `67ed2622e8eedf5a407d618bca2e7c9bbbdb43901737087847e4f87e9f4185be` | base |
| `check/return_rules.nv` | `9b12e44d1d2a6314403dd56e24dccd7bde603e3480d14a15d61410f2e9600e93` | base |
| `check/rules.nv` | `40aad8c2ae8fd99119b3f42b0c2d4897cd157d98992c49c8cfbc7ef047b018b6` | head |
| `check/run.nv` | `a9d98fe0b9fe539db7f4968cabcf364ef6463dd07aefbf9160b30470e0775697` | base |
| `check/scope.nv` | `474f8d8afe0456ebe6f4d4c25b2219262301e51febf6a6bc55a40502601fdcdd` | base |
| `check/slice.nv` | `fbd35cdb9e97776a31b4f40a3db467ac6439eafcda617b6854857448d64d2733` | base |
| `check/static_call.nv` | `d6f93ef65cfb1cdcfa1ec4a4b2a2e1c81719db968da0be49c93356a8b49a5816` | base |
| `check/strings.nv` | `5cfcff28c282029ddf65c6ada22fa54572425ea3942b3182cf0f81b82bb457ed` | base |
| `check/tail_rules.nv` | `cdf940e6fc16b694bbbfed531b31411f96c3e6def4b850ad151998cfc24de20c` | base |
| `check/try_rules.nv` | `b5b555b584ec35b5d4dd3980fa07a9fad187da54a4be03d205852128dd1b1990` | base |
| `check/type_args.nv` | `7d471038a4187b7fd6fb59507719af8d63c8e441933b40b0abc4e5bcd25bfa97` | base |
| `check/type_of.nv` | `0472a20f9ffb8d05b2433d8cd88ed4c2c9c3fbc79f00ccd34dd2c0c694612724` | backport |
| `check/typed_bind.nv` | `1c5f5a82e5f141a7df325663a7f03724695a5fbb8d904c848edd69b9eb959dee` | base |
| `check/typeref_rules.nv` | `91bc9c80666922f35bc323bdd77c3e92e59a7586b05408017418fa7c2ea5d03c` | base |
| `check/typing.nv` | `7ecb2c6869a061c8170b99eb6d62ee9df9e38b006de539e854225283ea76c03d` | merged |
| `check/unsafe.nv` | `6307e13b89b1320ac3bb9044028768e8fdaa89f00686f0f82da79f38d2999f56` | base |
| `check/variant_rules.nv` | `54dbf855e89ffb7a2858bcb1f0bdd4d326ef3d2d15e459f62a58ebd816e7c72e` | base |
| `check/with_typing.nv` | `65844ce3345aee217aac123090e5526b30697372b5be04b5234fef098721b9a3` | base |
| `diag/diag.nv` | `079d95247bc039c84429b43c54b49e78650165fd73fbbc609329fbb9c513b0d7` | base |
| `diag/diag_test.nv` | `e1a014e85502dfd02b3268a6703fcafe270aae1cee1666ff312bf42da0e017bf` | base |
| `emit_c/emit_applied_sum.nv` | `4aeed1d7c78e3231ef5a43b2f7c0dd8f91a47fb4312dfd918ea1e284e655947e` | base |
| `emit_c/emit_args.nv` | `30fd35c571ff5513fa0e2b08f1c96846bc7cf2bb9bfd8298f4f286f70a53a461` | base |
| `emit_c/emit_assert.nv` | `3650ae4dd621ce9947e4b420a6031203a419642e446e51c16dd0e5cab0d86f5e` | base |
| `emit_c/emit_c.nv` | `fb9961c1b353c65f705860afbf74a22d61be9a3050e6e25f6c9d579bc94d020b` | merged |
| `emit_c/emit_cast.nv` | `981d6aaa84c9a92f2fa2a818eff57a5b8621a55128e4345b264043def3ba8924` | base |
| `emit_c/emit_decls.nv` | `cad3a9edb817aaa15825c1a2afd30a1caee33eb3f30e783148b098122aab70c7` | head |
| `emit_c/emit_destructure.nv` | `74a93a626ceba85f365550cc15aaf6613ee45701c2ba30a67c35fddb39375246` | base |
| `emit_c/emit_embed.nv` | `43912559441ce15238b8df9f8ea16fb2eb2aad57fae32b00b72e25ffbacaad1a` | base |
| `emit_c/emit_expr.nv` | `94c03e052e2b67311990f638822499a405be74a331937ee79599cb4104d333c8` | head |
| `emit_c/emit_flow.nv` | `2c4e9c7aeb971158407b0e438489a7392e9e3fd921d32a0945095a1eaaba0add` | head |
| `emit_c/emit_handler.nv` | `92f5bfef32e30737abb870d06659a4909ef7e1b2129f835409ef02318e175285` | base |
| `emit_c/emit_instance_structs.nv` | `5bcb98e575c86edd7f02f8e36e15e3c294d043ac3a6e7513d14d6b185400da17` | base |
| `emit_c/emit_interp.nv` | `42aac49e4f8c6d2972a2e4c0da771c39ce05da62c80937186539569a846f97bb` | base |
| `emit_c/emit_is.nv` | `38b310faf62ecb499f1a33c52457434ac39ecb01f3cd53e1f7b3659734923208` | base |
| `emit_c/emit_match.nv` | `195eb385083dd93fa8766217305bdf622e71ab9c772f7873e5860a2910958c22` | base |
| `emit_c/emit_module_values.nv` | `843c948d468f8a8311a516343c0d1baa6c82b0e41b0c207fd1b24d21b2e3eaa9` | base |
| `emit_c/emit_never.nv` | `a30205a02e6d47327f19c8d5ce6a6104c35a36bb7fa811c55b74ae4d5821a5c6` | base |
| `emit_c/emit_option.nv` | `b314532e8c1dfbdca0662cb6855863d13e8fc486d90effd4c981bb6d3e49ba6e` | base |
| `emit_c/emit_place.nv` | `61dec9833efb24b071fff26cfd9eaa58648b68f2a22c69d20f7423211149a292` | base |
| `emit_c/emit_ptr.nv` | `8a8de5fc7def29e415fea7faa50869b449fa7468f251fb08549775d0abfbee32` | base |
| `emit_c/emit_requires.nv` | `63d13532b16b2310b3f3026a41746db705586f1af167356a679860a3120fc28f` | base |
| `emit_c/emit_slice.nv` | `55808554b4ce7b062fd4bd61149bbc2dfe6949c7399d1d8c9d266e9cecfb8f2e` | base |
| `emit_c/emit_spawn.nv` | `3cfc8d8366c54111c86eded00a9ba2a03e9e01135edc1ae3337f3848b8d968df` | base |
| `emit_c/shell.nv` | `3faa16e12f8a6bb4527e9c37a878567b7ac06e7b8c4898e4cabd91c58b581e39` | base |
| `emit_c/shell.tpl.c` | `27239b5221b4c68bab2c2fbba1ba657d5e4a508a8797e2db94d17b77a8b62469` | base |
| `lex/lex.nv` | `22053abe1c1bb395ee8b35f01c3189ee34ba4d399e469622cfff76290a6ff6a6` | head |
| `lex/lex_test.nv` | `7c5c3b54f236a6c93c4cbfcb2cfeb0317ab87a8a930f9463a0845609c61aa42d` | base |
| `lex/spelling.nv` | `1d18c1d55149209b0891f017fee11255c928bff9592ba7ba961dfd7be7a96f08` | head (new file) |
| `lower/ir.nv` | `0319d9d1fba4f924f354f2b309e3c969849980a6753d7cae043feb7a5589d2e9` | head |
| `lower/ir_cleanup.nv` | `bc260da22df07ef9cf38cbec0bc5ef9d995d390205de1d41daf676549ac48c74` | base |
| `lower/ir_match.nv` | `b8ef21fb82782276780ba9049ce31af7187c4b35460d2f32ccd5d86441e0829c` | base |
| `lower/ir_with.nv` | `b8760d4c7bfa7932969a15d4c6467427bad9b7e7ad3ad60e9fda11e125bcaff2` | base |
| `lower/lower_branch.nv` | `69eadc0e87a0798ef2f7dd86506c8f9099b74805c3ec4ba7d2e1ba959d09dfec` | base |
| `lower/lower_coalesce.nv` | `88dacbd39bb93abb12f99e246be8c57d7396875f25a04489314880e1c547a5e9` | base |
| `lower/lower_disj.nv` | `9c4cdc5a087fecab3d43233b45bbfc1bb24d97aa9a12e58f9247527f7d8db04b` | base |
| `lower/lower_hoist.nv` | `2d44ef0de3731764c61e2fa2cad90630b3e57017007ce790c6a139ce17b9de05` | base |
| `lower/lower_loop.nv` | `927379195d6612b4b095e815e42485defcd20bda49e71ac889fa5e31dc8d6ce4` | head |
| `lower/lower_match.nv` | `36a0c999e06001ad996ce23a902118ac459d3d8427a6951b069c75d6f11688d8` | base |
| `lower/lower_test.nv` | `33505a79e744c54f2f50a4266f4e29e169e29d23ac742c21cfc39b4ec6e5d05d` | base |
| `lower/lower_unsafe.nv` | `e15ad62bc778305014ae291900f865808a69977bb338d6ac79baf40e13c7e0e0` | base |
| `lower/lower_with.nv` | `72528678ab632258eec365de80915eb538bab2ccbbc248306bf1500a8b0e144d` | base |
| `lower/lowering.nv` | `0f9de0205a9e2ddda04b820e6d0afb3a2514e1ada4ea3657d72504ca7ed04218` | head |
| `main.nv` | `ab0c88b18a06c977b5baed557f6eb0ffec60ba6d7d4e0711ba6b878d37f27a1f` | base |
| `mono/mono.nv` | `20c669ed9a633074f96beff412ae38b833307182e1540cc2a03c8f26bbc69a11` | base |
| `names/names.nv` | `33f4f4a3c512a33b4888aef26f184d1155300b157b7f8a13695f37045839c5eb` | base |
| `parse/cast_chain.nv` | `feb7a95edd031f9e3992209a2986bf048f4edec6d643096f5889b6d6e6073700` | base |
| `parse/decls.nv` | `333e50210be5c4dc6839ba91a813c49de97430d4151fe7b0f3e0cf787bcfbe24` | base |
| `parse/expect.nv` | `abdd30c67b78d922529bc44e2b19061a8bd382d532f84db1cd3d1d84c324e176` | base |
| `parse/expr.nv` | `bae0128cf1acde49df58ced3dafb55a26a652fdaae1941c7d2283d4f879d1ae5` | head |
| `parse/handler.nv` | `1a02a3dc9625034291df96d01c0c99aeecee3b851eaf92fcbeebd59e3e65beb0` | base |
| `parse/parse.nv` | `8a2828d17afdb5ec5de44cc5593d8335f777f6c6f3188da49e52c7c5c155ee54` | head |
| `parse/parse_test.nv` | `53bcff76ec0121393889753e687763ff1dfd513519a392e8f7e172e8f4a5eafc` | base |
| `parse/pattern.nv` | `da38d94eb2b977b847d520adfbe1247adb27186b64e16ad68554d3a53d47fe10` | base |
| `parse/stmt.nv` | `b64ebdc1df1089caf6b9e70d9a52c078a08e5ff752ceabe7ff08ba2b4d9a7694` | head (new file) |
| `parse/type_decl.nv` | `861c6d76d148f897669cf35bfd13b11d87bb2ded3618e9083bf1481bbd710224` | head |
| `parse/type_ref.nv` | `80be47a1d9ea33d7ff4ee9f2deee61c084cce6e5793b672b17433d6d7596f73e` | base |
| `pipeline/arg_order_test.nv` | `3c4136c556db51f2050f35af51992b087292ab7f480cae15e667dbbe7de63843` | base |
| `pipeline/assoc_import_test.nv` | `5183d0fa54b4ee61bd912cf6f01466dfeffb44fdf9b2d9c5d7c04eb950977cb5` | base |
| `pipeline/cast_test.nv` | `329eec5827d4831e7d4d04427b5533aa356ee91fcc80d0ba76f32ae3d3f37fa6` | base |
| `pipeline/concurrency_test.nv` | `f47b4d5333fbe29041400155173b737c7908f59f61ae6eb62af97ff210253819` | base |
| `pipeline/const_expr_test.nv` | `0e77c3a91eb4fe08b282bc6ab918c851aeb804cc692cf64354c5c33535434114` | base |
| `pipeline/decls_test.nv` | `d94834410a57fc7bab5dcade78789865ade2cde3c0e1babf23543e3205bc1aaa` | base |
| `pipeline/derive_serde_test.nv` | `db8b1bf709ed672e676a16478ef1512204fe910ed1b4db7b0b85f452d8a849a4` | base |
| `pipeline/eq_door_test.nv` | `0f634be517de75f3da9a2b174595baafe4cbfcdf4dbfd8bc79233d62b1c4f2b3` | base |
| `pipeline/fluent_self_test.nv` | `7acd0e8fd25245f5fee4a2b40e452fbfece7797a2583f257436da962649e2332` | base |
| `pipeline/for_iter_test.nv` | `482f69f6a72ddb8aa194d083e0823303a8829307a7f026b65d899bdf1e77585f` | base |
| `pipeline/generic_body_test.nv` | `a886e3630e33134f924713916c44805a777c3fb4ad232642d980abc5de8109dc` | base |
| `pipeline/handed_default_test.nv` | `f2ebe96a9a210218aa42a8635c62f002c50dd29e26617e53084718c2f9fcdad9` | base |
| `pipeline/handed_fields_test.nv` | `f4e92ea9283586383fa81e2507a88fe33aae0a5708f5cbc115ed680475f682b6` | base |
| `pipeline/handed_free_test.nv` | `5d1dd5272629b0b3cacbc4dec3eb94e585c6c4c073ff897bc0fa962b4e92479d` | base |
| `pipeline/handed_plain_test.nv` | `d78a016bd48982896c5c0f8ffdacd857a232e5ca6775317a04cc85e827deacc6` | base |
| `pipeline/handed_self_test.nv` | `e0dd500bfe7526c9b385e987a6460e85b4606d4d004c1675a1c317d598e9a06d` | base |
| `pipeline/handed_sum_test.nv` | `712dd5f838516519246580c77fdc5d90bb9a951441b2116700d3c8ab1a780e5a` | base |
| `pipeline/handed_variant_test.nv` | `c34d7d66e6ca118d4630b0c3c010170a3c1481761ec7de847fc975269f11e33b` | base |
| `pipeline/if_value_test.nv` | `4bfee7280909fb29e46a9622be82b2659bbf7dc091a32e848a252f03ff0252ea` | base |
| `pipeline/iflet_test.nv` | `2a937c43d9542c6002c8024c1d4d47615d0980f7abd66947474c1cdebe6f21a8` | base |
| `pipeline/instance_scope_test.nv` | `306ce4114495a064bb3cef0d414dcc6636303250434936404ab3a505e6075c51` | base |
| `pipeline/instance_value_test.nv` | `f7ee7eb44f18923dee34ec7c5293a15eb4a17d9f925735ad566d6f7b2bd1fa56` | base |
| `pipeline/interp_test.nv` | `a36cc4fb892dd93433cbd4569a741db3bbaf0fbca1258d0fec58a1742a8ff76c` | base |
| `pipeline/match_applied_sum_test.nv` | `bd9fc7447a3e5ac41451cc28ad375737e1ceadc289ec6f5c2162121a2e00eddc` | base |
| `pipeline/match_value_test.nv` | `74b6dbc9b7d9eb320f90210f6de942578451c0896374a36be9b0013023ccb29c` | base |
| `pipeline/module_value_test.nv` | `244a78931123df2d6033974378dd2fa1c31f0878af2815d49068a2dcc28fbbf3` | base |
| `pipeline/mut_receiver_test.nv` | `2b28585d9c59ea91dd8755b4d1183de7e7668cea5b5f8a87524159cba604724d` | base |
| `pipeline/numeric_test.nv` | `2e523457732ea6a929ee6b8b811caae825e6de172533762657bb4d9b8f32ac07` | base |
| `pipeline/option_test.nv` | `6da1fc28d90677c0618d4a3c27b3293e1f664d29b24a36c9c803f2291c954922` | base |
| `pipeline/overload_test.nv` | `b6aab9edd37b4865006cbc169cee2da637710c3178de79a0fe0b159b170c8d41` | base |
| `pipeline/pipeline.nv` | `da06ccc50b6450d944fdb1568925e3e09ba7c39b0694adfc6b3b1c3f84dc7af4` | merged |
| `pipeline/pipeline_test.nv` | `ce00fbec7aa6609ad472d78842d70616a72fbf0df963a4f3fc580d59395bcfc0` | base |
| `pipeline/plain_overload_test.nv` | `6a01f2cad6c6f19e28f813edf2f7bb1fc81f03c506af1298f3a14a30859ef5f4` | base |
| `pipeline/prims_test.nv` | `0f71f8c579d846302a8350b9f2d0fb2bb889d01592f99ecd69a849856f07ac0b` | base |
| `pipeline/print_end_test.nv` | `0779a8927f3ab2fd5801a704fdae57507f75a64d5f7c3fe5ec0aaab0fe2d5cb1` | base |
| `pipeline/program.nv` | `addda20c4b27d532dc5031366592ab452e2e0274256649b841151d3d4a49fcc9` | merged |
| `pipeline/ptr_narrow_test.nv` | `6cd7b52d27c4a0e566c0555b311eced490a67a8e72d7a76b7d7a525abe19dfcf` | base |
| `pipeline/refusal_test.nv` | `fa135e0074d1f53aff98dc4a549d72583552f1dfc2ac00919af64765b0c149fe` | base |
| `pipeline/return_pos_test.nv` | `6f49ba5974cdd27844a744fded8de1c08793c5ccd07d67f680c9a52ac67d7988` | base |
| `pipeline/return_test.nv` | `54a023fe3fd1a491411a7f13156e05585480b9c600c4b84eb9641a0da569281c` | base |
| `pipeline/self_ctor_test.nv` | `97555f775fe8ba4348d2767a227bf6e5992583f272d67749e6a83d0d808d7ebd` | base |
| `pipeline/shell_overload_test.nv` | `009952dfa73565c5484add150c34b9f8a361c1b2988272cee61f31d153476e1b` | base |
| `pipeline/shell_stub_test.nv` | `7716df4ab0de20cc5df7cdb3b4e2ab0107aaec671024ac1f820b909ad9f0f4b5` | base |
| `pipeline/slice_eq_test.nv` | `5d7d4eed37787868a55a9d929a0993c182c6259b3f306c6fb3e4592c934f9409` | base |
| `pipeline/static_args_test.nv` | `252f0a306df21e60172b8061370e6031576d40e9a131b86374f18d9b790b8089` | base |
| `pipeline/str_ordering_test.nv` | `41c804f9f3591f16e8c7d85961f88409d74ffbec747af47da59298d3df14d68b` | base |
| `pipeline/subset_pattern_test.nv` | `30b1e7fefcf83c67e42ea936dbceb50c02ed339b977ad372d7aac0237ebbf26b` | base |
| `pipeline/subset_test.nv` | `fac3a778c252a1e5a1fd5bb35114edc635f9b08056767e206a1895066b4e6115` | base |
| `pipeline/target_row_test.nv` | `51d5797db518c827ac9b3fd4475caa81a8be5b2af1634d6b0b9a00141b263671` | base |
| `pipeline/try_test.nv` | `cf4fa80e83835758ee0a7fe6898a8a37557e1db7ebbb45edeb8dbd3cf7e413d6` | base |
| `pipeline/typeref_test.nv` | `cd28caa1b141f989855c856399c7db6ad36f02b3d00cf7aae8397ab0af387847` | base |
| `pipeline/unit_test.nv` | `033ef1df20047a86322cfed273a12c6b51e74e82cb386de41eaed28d2ed47eac` | base |
| `pipeline/value_recv_test.nv` | `eac105ca7e45f0d2fc16997df00d3e6edf10459f77065f54c4e6839aecd2e683` | base |
| `pipeline/variants_test.nv` | `68b3a40baa1bdd3073b14b1548c0cd9a540d46f885aed09b971f3163bd787c9d` | base |
| `pipeline/with_test.nv` | `153c396f2594f6d3f1babde8e97973e8b054484de25fbf4f93d2c6f2278fc156` | base |
| `resolve/resolve.nv` | `f201dba44b75646f2c9ac1b46a699e629b0ce3be427c82049afd5c040278e26f` | base |
| `resolve/resolve_test.nv` | `9aea9c68569e60b0cd9fa3b9fb2c231753b6e01229f0e0cc2f8a41bedb88d155` | base |
| `sem/any_is.nv` | `6342532e0bbf988a74ab6c670998e8d59492122880f3a810aee53e7e69e8456f` | base |
| `sem/binding.nv` | `34e3a81886b3ad7e21215da482f1daf277f1b3f7d8a4cde24d3ea7b138bbecce` | head |
| `sem/binding_test.nv` | `b2b58f187e1d4548beb28a79447cc35d5898fd86304a5f6157daa458c13a5992` | base |
| `sem/callables.nv` | `f6a39615406273e0848ad8a6cad94a9d7bd9b42d4a9baf6caa664e5535956be6` | merged |
| `sem/channel.nv` | `c6f8358aec0182dace3386cbc616b4e3e187a3ec5e4f846932a4f5a519f0bd44` | base |
| `sem/coerce.nv` | `a2c36c4bdf8ccd981f71dd1fae7a5b1a0c9b8ae302a989861712fb84d1542cac` | base |
| `sem/collect.nv` | `d3ea5cd0aa44128fb022cffe56e507f0ad3700849ed6a22078f7c2ae4dabd7f8` | merged |
| `sem/constexpr.nv` | `57ccafa275e372aa8687e6e747389e76a6b45b0194dd409368c321a8fc0e3a51` | base |
| `sem/defs.nv` | `8dce375ddfb6edd8ca02371df6f80e5f0a0da3e3464e71117c825e3f3144d38f` | base |
| `sem/effects.nv` | `c3a94f9913ba6d9e0f41d2085ef553647de98a74c85ae6a839e22cf5e62a7d03` | base |
| `sem/embeds.nv` | `930a12787515d2f234de7946913c5c9fa36e0964d7b7bf8534b70978b669a284` | base |
| `sem/file_shape.nv` | `1dac531fd0c4f219a9ef09df333dddf7d722ae295b17ff8d2410991929695503` | base |
| `sem/handed_bodies.nv` | `50386e17b5485eee5daacbf62b6dbb35d0b601132d5fa324d025b3c0d5c9d59a` | base |
| `sem/handed_fields.nv` | `0c86ae486c7ca4acbd83a4c536254ff20e6ea121c88052c2cc279768617bab54` | base |
| `sem/harvest.nv` | `65aded1e9bde67f6eb2ad864b246d2ae143ed1e59bf5b0faa1855e1c17268637` | merged |
| `sem/interop.nv` | `d8e7c9ea8a917cfccd770d7bb356c9b57880c87147c7c287a6002a68a732d73d` | base |
| `sem/literal_spelling.nv` | `8758b9e95c1a529e049ea5ac8da1b471cddfe9993fcdffac6021726b13a5cf7d` | base |
| `sem/litpool.nv` | `60c86fb6b5097dd233fc62caf76556f36c84c1c251fceafa64eaf9743195f25c` | base |
| `sem/mangle.nv` | `2a7dc4788a5c82aaad14ade97a7c56dceb4b80393c4116e92f7832e374acc997` | merged |
| `sem/mangle_home.nv` | `9192897a424d0a14bab0603c71977595f6993c989108d9174ac18acbc53804c3` | base |
| `sem/mangle_instance.nv` | `2a0e5afba6c2eec027f441cc91b7b220112faab8b7644ea04db7a39a31f60f73` | head |
| `sem/mangle_spelling.nv` | `b0dea75419a06e2846d798758774eb40da9fc3b3121ca818fd7473b3c5feb41b` | base |
| `sem/mangle_test.nv` | `c76894f874b193560d2e0fc5c1d7360658c5e0cf0a5f080cca506417e218cc4c` | base |
| `sem/node_questions.nv` | `c8de5b5ad067027c36cff32ded515e3cca0e1d9c9f834924a7ac0883a1cff256` | base |
| `sem/pattern.nv` | `c482703987dbddfd51618c5c032621d18f88512f5595729be91a3335d09f1c9e` | base |
| `sem/protocols.nv` | `4634564f970296459221ca88d738b2401b6233caa7e68a96550124acc40765c2` | base |
| `sem/sem.nv` | `7d662296059050372a6d4bd245aed890558d23ef97f231b782721e22d77b6851` | base |
| `sem/slice.nv` | `a05793cab680dbcc1c3ed6cbc01af8862aaea83de322544244441820a5f3d8fc` | base |
| `sem/slots.nv` | `532922c25b290e41e8eb2ecc245ea3d482aa67e86a5c245c86df2acd798b0904` | head |
| `sem/slots_decl.nv` | `18bcc90c68e305919d7cf3124ab4861f47b402cf362d649c739df3d0f7558a14` | base |
| `sem/type_shape.nv` | `d45b0e3000d6f6d6073ec926fee41f7a293f3dabbf07c8cc6bc66b138163f421` | head |
| `sem/typeref.nv` | `2bf7c153d1fb890491336ffc918aee8bbb622a1dabb98ea7a483ea7ece0b462d` | head |
| `sem/value_recv.nv` | `560fb25ad7c5a636b4e02634a9344df22702ac494e8e9574bfa4fcdc752ae5f7` | base |
| `source/source.nv` | `7b1b858d5cf310cf3602d056740595f2a166c9d3daeb10227e6968aaacb6083c` | base |
| `source/source_test.nv` | `c6d905b611b3b536600b8c8aece6ed9e623e3c039b0d916691a8cfef80f1d0b5` | base |
| `tree/tree.nv` | `70b09c9448aea082df97b50c48f351b3c559cbccd4e4cda2f25dc9b734def7b9` | head |
| `types/types.nv` | `6730a6b8da2bc9c125b46b791c8a723053c7fe25a38ca4ed05ab7684be12932e` | head |
| `types/types_test.nv` | `808c50eede409f77b27841128eb21b5684fd4389053cf3741ffa44014d7c0933` | base |
