const supertest = require('supertest');
const expect = require('chai').expect;
const mocha = require('mocha');
const tv4 = require('tv4');
const fs = require('fs');
const pm = require('../pm');
const stripe = require('../../helpers/stripe');

const baseUrl = supertest(pm.environment.get('server'));

const apiEndPoint = '/cart/checkout';


var response;
var responseBody;


const post = async function (request_body) {
    return baseUrl
        .post(pm.substitute(apiEndPoint))
        .set('Accept', 'application/json')
        .set('Content-Type', 'application/json')
        .set('Authorization', pm.substitute('Bearer {{user_token}}'))

        .send(pm.substitute(request_body));
};

const get = async function (request_body) {
    return baseUrl
        .get(pm.substitute(apiEndPoint))

        .set('Authorization', pm.substitute('Bearer {{user_token}}'))

        .set('Accept', 'application/json')
        .send();
};

let requestBody = `{
	"amount": 6120,
	"method": {
		"type" : "Card",
		"provider": "Stripe",
		"token" : "{{last_credit_card_token}}",
		"save_payment_method": false,
		"set_default": false
	}
}`;


describe('User - checkout new order', function () {
    before(async function () {
        await stripe.getToken();
        response = await post(requestBody);
        console.log(response.request.header);
        console.log(response.request.url);
        console.log(response.request._data);
        console.log(response.request.method);
        responseBody = JSON.stringify(response.body);
        //console.log(pm);
        console.log(response.status);
        console.log(responseBody);
    });

    after(async function () {
        // add after methods


        pm.environment.set("last_cart_id", JSON.parse(responseBody).id)

    });

    it("should be 200", function () {
        expect(response.status).to.equal(200);
    })


});

            